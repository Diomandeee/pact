//! The diagram, and its validation.
//!
//! Everything here is decidable by looking at the graph. No simulation runs,
//! nothing is executed, and the cost is linear in nodes times edges. That is
//! the point of the cheap tier: it can run on every edit, so a defect is found
//! by the author rather than by whoever reads the report six weeks later.

use crate::edge::{ResourceEdge, StateEdge};
use crate::node::{Kind, Node};
use std::collections::{BTreeMap, HashMap};

/// Inflow-to-outflow ratio at which a pool is reported as unbounded.
///
/// A judgement call, which is exactly why it is a named constant rather than a
/// literal inside a condition. The same number lives in `spec/invariants.toml`
/// as `pressure_ratio`, and `scripts/canaries.sh` checks the two agree.
///
/// The duplication is deliberate. INV-1 forbids `pact-core` any I/O, so the
/// engine cannot read the spec file, so the only honest options are to
/// duplicate the number and check it externally, or to let the spec file
/// describe a constant nothing enforces. The first is the one with a detector.
pub const PRESSURE_RATIO: usize = 3;

/// A structural problem with the diagram, found before any simulation runs.
///
/// These are the cheap tier. Everything here is decidable by looking at the
/// graph, which means it costs nothing and can run on every edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Defect {
    /// An edge names a node that does not exist. Both endpoints are reported
    /// so the finding is actionable without re-reading every edge by hand.
    DanglingEdge {
        /// The edge's source, whether or not it is the missing one.
        from: String,
        /// The edge's target, whether or not it is the missing one.
        to: String,
        /// The endpoint that was never added.
        missing: String,
    },
    /// Two nodes share an id.
    DuplicateNode {
        /// The id claimed more than once.
        id: String,
    },
    /// A converter with no way to get resources. It can never fire, and it
    /// will never say so: it will simply sit there looking configured.
    /// Nine skills in the real corpus are in exactly this state.
    StarvedConverter {
        /// The converter nothing feeds.
        id: String,
    },
    /// A pool that fills faster than it empties. In a game economy this is the
    /// classic broken-currency signature; here it is a type with many
    /// producers and few consumers.
    UnboundedPool {
        /// The pool under pressure.
        id: String,
        /// Resource edges arriving.
        inflow: usize,
        /// Resource edges leaving.
        outflow: usize,
    },
    /// A source feeding nothing.
    OrphanSource {
        /// The source wired to no target.
        id: String,
    },
    /// Resources can reach here and never leave. Legitimate for a Drain,
    /// a defect for anything else.
    Sink {
        /// The node where work arrives and stops.
        id: String,
    },
}

/// The diagram.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    nodes: Vec<Node>,
    index: HashMap<String, usize>,
    /// Resource connections, in insertion order. Order is load-bearing:
    /// `Effect::Modifier` addresses an edge by its index here.
    pub resources: Vec<ResourceEdge>,
    /// State connections, in insertion order. No resources move along these.
    pub states: Vec<StateEdge>,
}

impl Graph {
    /// An empty diagram.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node. Later duplicates are kept so `validate` can report them
    /// rather than silently discarding one; a builder that swallows a
    /// duplicate id hides a real authoring mistake.
    pub fn add(&mut self, node: Node) -> &mut Self {
        let id = node.id.clone();
        self.nodes.push(node);
        self.index.entry(id).or_insert(self.nodes.len() - 1);
        self
    }

    /// Add a resource connection.
    pub fn connect(&mut self, edge: ResourceEdge) -> &mut Self {
        self.resources.push(edge);
        self
    }

    /// Add a state connection.
    pub fn influence(&mut self, edge: StateEdge) -> &mut Self {
        self.states.push(edge);
        self
    }

    /// The node with this id. When an id was added twice, this is the first
    /// one, which is also the copy `validate` analyses.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.index.get(id).map(|i| &self.nodes[*i])
    }

    /// Every node in insertion order, duplicates included. Duplicates are kept
    /// rather than collapsed so `validate` can report them.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Structural defects, found without running anything.
    #[must_use]
    pub fn validate(&self) -> Vec<Defect> {
        let mut defects = Vec::new();

        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for node in &self.nodes {
            *seen.entry(node.id.as_str()).or_insert(0) += 1;
        }
        for (id, count) in seen {
            if count > 1 {
                defects.push(Defect::DuplicateNode { id: id.to_string() });
            }
        }

        for edge in &self.resources {
            for endpoint in [&edge.from, &edge.to] {
                if self.node(endpoint).is_none() {
                    defects.push(Defect::DanglingEdge {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                        missing: endpoint.clone(),
                    });
                }
            }
        }
        for edge in &self.states {
            for endpoint in [&edge.from, &edge.to] {
                if self.node(endpoint).is_none() {
                    defects.push(Defect::DanglingEdge {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                        missing: endpoint.clone(),
                    });
                }
            }
        }

        for (position, node) in self.nodes.iter().enumerate() {
            // A duplicated id is reported once, above. Analysing both copies
            // would report every downstream defect twice for what the author
            // sees as one node, and a report that double-counts is a report
            // people learn to discount.
            if self.index.get(node.id.as_str()) != Some(&position) {
                continue;
            }

            let inflow = self.resources.iter().filter(|e| e.to == node.id).count();
            let outflow = self.resources.iter().filter(|e| e.from == node.id).count();

            match node.kind {
                Kind::Converter { .. } if inflow == 0 => {
                    defects.push(Defect::StarvedConverter {
                        id: node.id.clone(),
                    });
                }
                Kind::Source { .. } if outflow == 0 => {
                    defects.push(Defect::OrphanSource {
                        id: node.id.clone(),
                    });
                }
                // Pressure: many producers, few consumers.
                Kind::Pool { .. } if outflow > 0 && inflow >= outflow * PRESSURE_RATIO => {
                    defects.push(Defect::UnboundedPool {
                        id: node.id.clone(),
                        inflow,
                        outflow,
                    });
                }
                _ => {}
            }

            // Sink is about a node's position, not its kind, so it is checked
            // outside the match. Work arriving somewhere with no way out is
            // destroyed whether that somewhere is a pool, a delay holding it
            // forever, a gate with nothing to distribute to, or a converter
            // emitting into nothing.
            //
            // A Drain is exempt because destroying resources is its job. A
            // Source is exempt because a source with an inflow is already
            // reported as an orphan when it has no outflow, and one node
            // producing two defects for one mistake is noise.
            let terminal = inflow > 0 && outflow == 0;
            let may_be_terminal = matches!(node.kind, Kind::Drain | Kind::Source { .. });
            if terminal && !may_be_terminal {
                defects.push(Defect::Sink {
                    id: node.id.clone(),
                });
            }
        }
        defects
    }

    /// Outgoing resource edges, with their index, so a Modifier can address
    /// a specific edge by position.
    #[must_use]
    pub fn outgoing(&self, id: &str) -> Vec<(usize, &ResourceEdge)> {
        self.resources
            .iter()
            .enumerate()
            .filter(|(_, e)| e.from == id)
            .collect()
    }

    /// Incoming resource edges, with their index.
    #[must_use]
    pub fn incoming(&self, id: &str) -> Vec<(usize, &ResourceEdge)> {
        self.resources
            .iter()
            .enumerate()
            .filter(|(_, e)| e.to == id)
            .collect()
    }
}
