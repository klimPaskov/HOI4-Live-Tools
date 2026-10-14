use hoi4_live_native::graph_kernel::{Graph, Limits, Visit};
use std::collections::BTreeSet;
fn set(ns: &[u32]) -> BTreeSet<u32> {
    ns.iter().copied().collect()
}
fn limits() -> Limits {
    Limits {
        nodes: 64,
        edges: 256,
        depth: 64,
        work: 4096,
    }
}
fn graph(rows: &[(u32, Vec<u32>)]) -> Graph {
    Graph::new(&rows.iter().map(|(n, _)| *n).collect(), rows, limits()).unwrap()
}
#[test]
fn disconnected_bridge_cycle_and_exclusions() {
    let g = graph(&[
        (1, vec![2, 3]),
        (2, vec![1, 3]),
        (3, vec![1, 2, 4]),
        (4, vec![3]),
        (5, vec![]),
    ]);
    assert_eq!(
        g.components(&set(&[1, 2, 3, 4, 5]), &set(&[])).unwrap(),
        vec![set(&[1, 2, 3, 4]), set(&[5])]
    );
    assert_eq!(
        g.components(&set(&[1, 2, 3, 4, 5]), &set(&[3])).unwrap(),
        vec![set(&[1, 2]), set(&[4]), set(&[5])]
    );
}
#[test]
fn unknown_and_untraversable_nodes_never_pass() {
    let g = graph(&[(1, vec![2]), (2, vec![1, 3]), (3, vec![2])]);
    let s = g.bfs(&set(&[1, 3]), &set(&[]), &set(&[1]), 8).unwrap();
    assert_eq!(s.visits.len(), 1);
    assert!(!s.depth_truncated);
    for unknown in [set(&[1, 99]), set(&[99])] {
        assert_eq!(
            g.components(&unknown, &set(&[])).unwrap_err().code,
            "invalid_graph"
        );
    }
    assert!(g.components(&set(&[1]), &set(&[99])).is_err());
}
#[test]
fn missing_unknown_excluded_and_nontraversable_seeds_refuse() {
    let g = graph(&[(1, vec![2]), (2, vec![1])]);
    for seeds in [set(&[]), set(&[99]), set(&[2])] {
        assert!(g.bfs(&set(&[1]), &set(&[]), &seeds, 1).is_err());
    }
    assert!(g.bfs(&set(&[1, 2]), &set(&[1]), &set(&[1]), 1).is_err());
}
#[test]
fn tie_source_propagates_after_complete_layer_independent_of_input_order() {
    let rows = vec![
        (9, vec![4]),
        (4, vec![9, 3]),
        (3, vec![4, 2, 5]),
        (2, vec![3, 1]),
        (1, vec![2]),
        (5, vec![3]),
    ];
    let g = graph(&rows);
    let mut reversed = rows.clone();
    reversed.reverse();
    for (_, ns) in &mut reversed {
        ns.reverse();
    }
    let domain = set(&[1, 2, 3, 4, 5, 9]);
    let a = g.bfs(&domain, &set(&[]), &set(&[1, 9]), 4).unwrap();
    assert_eq!(
        a,
        graph(&reversed)
            .bfs(&domain, &set(&[]), &set(&[9, 1]), 4)
            .unwrap()
    );
    assert_eq!(
        a.visits[&3],
        Visit {
            distance: 2,
            source: 1
        }
    );
    assert_eq!(
        a.visits[&5],
        Visit {
            distance: 3,
            source: 1
        }
    );
}
#[test]
fn depth_truncation_is_explicit_and_zero_depth_is_valid() {
    let g = graph(&[(1, vec![2]), (2, vec![1, 3]), (3, vec![2])]);
    let all = set(&[1, 2, 3]);
    let zero = g.bfs(&all, &set(&[]), &set(&[1]), 0).unwrap();
    assert_eq!(zero.visits.len(), 1);
    assert!(zero.depth_truncated);
    let one = g.bfs(&all, &set(&[]), &set(&[1]), 1).unwrap();
    assert_eq!(one.visits.len(), 2);
    assert!(one.depth_truncated);
    assert!(
        !g.bfs(&all, &set(&[]), &set(&[1]), 2)
            .unwrap()
            .depth_truncated
    );
}
#[test]
fn malformed_rows_edges_and_unknown_endpoints_refuse() {
    for rows in [
        vec![(1, vec![2]), (2, vec![])],
        vec![(1, vec![2, 2]), (2, vec![1])],
        vec![(1, vec![1]), (2, vec![])],
        vec![(1, vec![]), (1, vec![])],
        vec![(1, vec![99]), (2, vec![])],
    ] {
        assert_eq!(
            Graph::new(&set(&[1, 2]), &rows, limits())
                .err()
                .unwrap()
                .code,
            "invalid_graph"
        );
    }
    assert_eq!(
        Graph::new(&set(&[1, 2]), &[(1, vec![])], limits())
            .err()
            .unwrap()
            .code,
        "graph_bound"
    );
}
#[test]
fn node_edge_depth_and_constructor_work_limits_fail_explicitly() {
    let rows = [(1, vec![2]), (2, vec![1])];
    for l in [
        Limits {
            nodes: 1,
            ..limits()
        },
        Limits {
            edges: 1,
            ..limits()
        },
        Limits {
            work: 1,
            ..limits()
        },
        Limits {
            depth: 8193,
            ..limits()
        },
        Limits {
            nodes: 8193,
            ..limits()
        },
    ] {
        assert_eq!(
            Graph::new(&set(&[1, 2]), &rows, l).err().unwrap().code,
            "graph_bound"
        );
    }
    let g = graph(&rows);
    assert_eq!(
        g.bfs(&set(&[1, 2]), &set(&[]), &set(&[1]), 65)
            .unwrap_err()
            .code,
        "graph_bound"
    );
}
#[test]
fn operation_work_includes_domain_and_traversal_not_partial_success() {
    // Use isolated exclusions to consume validation work without reducing walk.
    let rows = [(1, vec![2]), (2, vec![1]), (3, vec![])];
    let g = Graph::new(
        &set(&[1, 2, 3]),
        &rows,
        Limits {
            work: 7,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(
        g.components(&set(&[1, 2, 3]), &set(&[3])).unwrap_err().code,
        "graph_bound"
    );
    assert_eq!(
        g.bfs(&set(&[1, 2, 3]), &set(&[3]), &set(&[1]), 2)
            .unwrap_err()
            .code,
        "graph_bound"
    );
}
