//! Independent bounded public-topology algorithms. Consulted components and
//! multi-source BFS ideas in smerss/Clausewitz-MCP 97c85437081904d5ad07bae1623ec58d1c0a5b9e,
//! calling/doctrine.py and calling/ragemicro.py (MIT; LICENSE.clausewitz).
//! No upstream code copied. Graph disconnection is not supply/action evidence.
use crate::error::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
pub type Node = u32;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    /// Directed adjacency entries (a symmetric edge consumes two).
    pub edges: usize,
    pub depth: u32,
    /// Node visits plus adjacency inspections, separately per operation.
    pub work: usize,
}
impl Limits {
    fn validate(self) -> Result<Self> {
        if self.nodes == 0
            || self.nodes > 8192
            || self.edges > 65536
            || self.depth > 8192
            || self.work == 0
            || self.work > 1_048_576
        {
            return Err(err("graph_bound", "Invalid graph policy bounds"));
        }
        Ok(self)
    }
}
fn err(code: &'static str, msg: &'static str) -> Error {
    Error::new(code, msg)
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn add(&mut self) -> Result<()> {
        self.used = self
            .used
            .checked_add(1)
            .ok_or_else(|| err("graph_bound", "Work overflow"))?;
        if self.used > self.max {
            return Err(err("graph_bound", "Graph work bound exceeded"));
        }
        Ok(())
    }
}
pub struct Graph {
    adjacency: BTreeMap<Node, BTreeSet<Node>>,
    limits: Limits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visit {
    pub distance: u32,
    pub source: Node,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Search {
    pub visits: BTreeMap<Node, Visit>,
    pub depth_truncated: bool,
}
impl Graph {
    /// Every adjacency node/endpoint must be in the explicitly proven set.
    /// Isolated proven nodes still require an empty adjacency row.
    pub fn new(
        proven: &BTreeSet<Node>,
        rows: &[(Node, Vec<Node>)],
        limits: Limits,
    ) -> Result<Self> {
        let limits = limits.validate()?;
        if proven.len() > limits.nodes || rows.len() != proven.len() {
            return Err(err(
                "graph_bound",
                "Node count or complete row coverage required",
            ));
        }
        let mut adjacency = BTreeMap::new();
        let mut edges = 0usize;
        let mut budget = Budget {
            used: 0,
            max: limits.work,
        };
        for (node, neighbours) in rows {
            budget.add()?;
            if !proven.contains(node) || adjacency.contains_key(node) {
                return Err(err("invalid_graph", "Unknown or duplicate adjacency node"));
            }
            let mut ns = BTreeSet::new();
            for n in neighbours {
                budget.add()?;
                edges = edges
                    .checked_add(1)
                    .ok_or_else(|| err("graph_bound", "Edge count overflow"))?;
                if edges > limits.edges {
                    return Err(err("graph_bound", "Edge bound exceeded"));
                }
                if n == node || !proven.contains(n) || !ns.insert(*n) {
                    return Err(err("invalid_graph", "Unknown, duplicate or self edge"));
                }
            }
            adjacency.insert(*node, ns);
        }
        for (node, ns) in &adjacency {
            for n in ns {
                budget.add()?;
                if !adjacency.get(n).is_some_and(|back| back.contains(node)) {
                    return Err(err("invalid_graph", "Symmetric adjacency required"));
                }
            }
        }
        Ok(Self { adjacency, limits })
    }
    fn domain(
        &self,
        traversable: &BTreeSet<Node>,
        excluded: &BTreeSet<Node>,
        budget: &mut Budget,
    ) -> Result<BTreeSet<Node>> {
        for n in traversable.iter().chain(excluded) {
            budget.add()?;
            if !self.adjacency.contains_key(n) {
                return Err(err("invalid_graph", "Unknown domain node"));
            }
        }
        Ok(traversable.difference(excluded).copied().collect())
    }
    /// Components and their nodes are ordered by smallest node ID.
    pub fn components(
        &self,
        traversable: &BTreeSet<Node>,
        excluded: &BTreeSet<Node>,
    ) -> Result<Vec<BTreeSet<Node>>> {
        let mut budget = Budget {
            used: 0,
            max: self.limits.work,
        };
        let domain = self.domain(traversable, excluded, &mut budget)?;
        let mut unseen = domain.clone();
        let mut result = Vec::new();
        while let Some(start) = unseen.first().copied() {
            unseen.remove(&start);
            let mut pending = BTreeSet::from([start]);
            let mut component = BTreeSet::new();
            while let Some(node) = pending.pop_first() {
                budget.add()?;
                component.insert(node);
                for n in &self.adjacency[&node] {
                    budget.add()?;
                    if domain.contains(n) && unseen.remove(n) {
                        pending.insert(*n);
                    }
                }
            }
            result.push(component);
        }
        Ok(result)
    }
    /// Shortest distance; equal-distance ties choose the smallest source ID.
    /// A complete breadth layer is resolved before its next layer propagates.
    pub fn bfs(
        &self,
        traversable: &BTreeSet<Node>,
        excluded: &BTreeSet<Node>,
        seeds: &BTreeSet<Node>,
        depth: u32,
    ) -> Result<Search> {
        if depth > self.limits.depth {
            return Err(err("graph_bound", "Depth bound exceeded"));
        }
        let mut budget = Budget {
            used: 0,
            max: self.limits.work,
        };
        let domain = self.domain(traversable, excluded, &mut budget)?;
        if seeds.is_empty() || !seeds.is_subset(&domain) {
            return Err(err(
                "invalid_graph",
                "Nonempty proven traversable seeds required",
            ));
        }
        let mut frontier: BTreeMap<Node, Node> = seeds.iter().map(|n| (*n, *n)).collect();
        let mut visits = BTreeMap::new();
        let mut distance = 0u32;
        let mut truncated = false;
        while !frontier.is_empty() {
            for (node, source) in &frontier {
                visits.insert(
                    *node,
                    Visit {
                        distance,
                        source: *source,
                    },
                );
            }
            let mut next = BTreeMap::new();
            for (node, source) in &frontier {
                budget.add()?;
                for n in &self.adjacency[node] {
                    budget.add()?;
                    if !domain.contains(n) || visits.contains_key(n) {
                        continue;
                    }
                    if distance == depth {
                        truncated = true;
                        continue;
                    }
                    next.entry(*n)
                        .and_modify(|old: &mut Node| *old = (*old).min(*source))
                        .or_insert(*source);
                }
            }
            if next.is_empty() {
                break;
            }
            distance = distance
                .checked_add(1)
                .ok_or_else(|| err("graph_bound", "Distance overflow"))?;
            frontier = next;
        }
        Ok(Search {
            visits,
            depth_truncated: truncated,
        })
    }
}
