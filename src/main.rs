//! `covlizer` command line binary.

use clap::{Parser, ValueEnum};
use itertools::Itertools;
use petgraph::algo::greedy_feedback_arc_set;
use petgraph::dot::Dot;
use petgraph::graph::EdgeIndex;
use petgraph::prelude::DiGraphMap;
use petgraph::prelude::Graph;
use petgraph::prelude::NodeIndex;
use petgraph::prelude::StableGraph;
use petgraph::visit::EdgeRef;
use ptree::graph::print_graph;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct Args {
    /// Input JSON file used to parse stacks
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Output DOT file
    #[arg(long)]
    out_dot: Option<String>,

    /// Output tree to stdout
    #[arg(long, action)]
    out_tree: bool,

    /// Graph prune strategy
    #[arg(short, long, value_enum, default_value_t=PruneOpt::All)]
    prune: PruneOpt,

    /// Target nodes to match
    #[arg(short, long, num_args = 0.., value_delimiter = ',')]
    targets: Vec<String>,
}

#[derive(ValueEnum, Clone, Debug, Eq, PartialEq)]
enum PruneOpt {
    /// Both paths after target nodes and before the first common node are pruned
    All,

    /// Keep all nodes for output graph
    Pass,
}

fn output(dig: DiGraphMap<&str, &str>, out_dot: Option<String>, out_tree: bool) {
    out_dot.map(|out| {
        let name = std::path::Path::new(&out);
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(name)
            .unwrap();
        file.write_all(Dot::new(&dig).to_string().as_bytes())
            .unwrap();
    });

    if out_tree {
        // Convert to a directed acyclic graph by removing back-edges.
        let g = dig.into_graph::<u32>();
        let mut sg: StableGraph<&str, &str> = StableGraph::from(g);
        let fas: Vec<EdgeIndex> = greedy_feedback_arc_set(&sg).map(|e| e.id()).collect();
        //println!("{:?}", fas);
        for edge_id in fas {
            sg.remove_edge(edge_id);
        }

        let dag = Graph::from(sg);
        //println!("{:?}", dag);
        let neighbors: HashSet<u32> = HashSet::from_iter(
            dag.node_indices()
                .into_iter()
                .map(|n| dag.neighbors(n))
                .flatten()
                .map(|i| i.index() as u32),
        );
        let roots = dag
            .node_indices()
            .into_iter()
            .filter(|n| !neighbors.contains(&(n.index() as u32)))
            .collect_vec();
        for root in roots.iter() {
            let dag_root = dag
                .node_indices()
                .into_iter()
                .position(|n| root.index() == n.index())
                .unwrap();
            print_graph(&dag, NodeIndex::new(dag_root)).unwrap();
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let json_str = std::fs::read_to_string(args.file)?;
    let json: serde_json::Value = serde_json::from_str(json_str.as_str())?;

    let mut nodes_by_targets: HashMap<String, HashSet<String>> = HashMap::new();
    let mut neighbours_by_nodes: HashMap<String, HashSet<String>> = HashMap::new();
    let mut stacks_by_targets: HashMap<String, HashSet<String>> = HashMap::new();
    let mut dig = DiGraphMap::<&str, &str>::new();
    for (stack, _) in json.as_object().unwrap() {
        // If more than one target is in the same stack,
        // pick the last to store the largest number of nodes reached.
        let maybe_target = args
            .targets
            .iter()
            .filter(|target| stack.contains(target.to_owned()))
            .last();
        if maybe_target.is_none() {
            continue;
        }

        let target = maybe_target.unwrap();
        if !nodes_by_targets.contains_key(target) {
            nodes_by_targets.insert(target.to_owned(), HashSet::new());
        }

        let stack_parts = stack.split("\x1f");
        let target_nodes = nodes_by_targets.get_mut(target).unwrap();
        let mut is_target_matched = false;
        for (node1, node2) in stack_parts
            .collect::<Vec<&str>>()
            .iter()
            .rev()
            .tuple_windows()
        {
            if !neighbours_by_nodes.contains_key(*node1) {
                neighbours_by_nodes.insert(node1.to_string(), HashSet::new());
            }
            neighbours_by_nodes
                .get_mut(*node1)
                .unwrap()
                .insert(node2.to_string());

            if !is_target_matched {
                target_nodes.insert(node1.to_string());
                target_nodes.insert(node2.to_string());
            }
            if args.prune == PruneOpt::All && node2 == target {
                is_target_matched = true;
            }
        }

        if !stacks_by_targets.contains_key(target) {
            stacks_by_targets.insert(target.to_owned(), HashSet::new());
        }
        stacks_by_targets
            .get_mut(target)
            .unwrap()
            .insert(stack.to_string());
    }

    // Compute intersections between stacks of distinct target nodes.
    // If there's only a single target, then we build a graph
    // of all stacks containing that target.
    let mut targets_intersection: HashSet<String> = HashSet::new();
    if args.targets.len() > 1 {
        args.targets.iter().for_each(|target| {
            if nodes_by_targets.contains_key(target) {
                let nodes = nodes_by_targets.get(target).unwrap();
                if targets_intersection.is_empty() {
                    nodes.iter().for_each(|node| {
                        targets_intersection.insert(node.to_owned());
                    });
                } else {
                    targets_intersection =
                        targets_intersection.intersection(nodes).cloned().collect();
                }
            }
        });
    }
    //println!("{:?}", targets_intersection);

    if args.targets.is_empty() {
        for (stack, _) in json.as_object().unwrap() {
            let stack_parts = stack.split("\x1f");
            for (node1, node2) in stack_parts
                .collect::<Vec<&str>>()
                .iter()
                .rev()
                .tuple_windows()
            {
                dig.add_edge(node1, node2, "");
            }
        }
    } else {
        // Attempt to minimize the number of roots generated from prunning,
        // by keeping track of nodes that have at least one non-intersecting child node.
        // These parent nodes will be "stitched" to pruned child nodes.
        let mut stitches: HashSet<String> = HashSet::new();

        for (target, stacks) in stacks_by_targets.iter() {
            for stack in stacks.iter() {
                let stack_parts = stack.split("\x1f");
                let nodes = stack_parts.collect::<Vec<&str>>();
                for (node1, node2) in nodes.iter().rev().tuple_windows() {
                    if args.prune == PruneOpt::All {
                        if targets_intersection.contains(*node1) {
                            if neighbours_by_nodes.contains_key(*node1)
                                && neighbours_by_nodes
                                    .get(*node1)
                                    .unwrap()
                                    .iter()
                                    .filter(|n| !targets_intersection.contains(*n))
                                    .count()
                                    > 0
                            {
                                stitches.insert(node1.to_string());
                            } else if targets_intersection.contains(*node2)
                                && !stitches.contains(*node2)
                            {
                                //println!("{} -> {}", node1, node2);
                                continue;
                            }
                        }
                    }

                    dig.add_edge(node1, node2, "");

                    if args.prune == PruneOpt::All && node2 == target {
                        // Skip children of this target.
                        break;
                    }
                }
            }
        }
    }

    output(dig, args.out_dot, args.out_tree);

    Ok(())
}
