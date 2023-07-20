pub mod offsets;
pub mod state;
pub mod extend;
pub mod scoring;
pub mod queue;
pub mod alignment;
pub mod visited;

use crate::graphs::{AlignableGraph, NodeIndexType};
use crate::aligner::offsets::OffsetType;
use crate::aligner::state::{AlignState, StateTreeNode, Backtrace, TreeIndexType};
use crate::aligner::scoring::AlignmentCosts;
use crate::aligner::queue::AlignStateQueue;
use crate::aligner::extend::PathExtender;

use crate::debug::DebugOutputWriter;
use crate::debug::messages::DebugOutputMessage;

pub use alignment::{AlignedPair, Alignment};

enum ExtendResult<Ix: TreeIndexType> {
    NewExtendedNodes(Vec<Ix>),
    ReachedEnd(Ix)
}

use ExtendResult::{NewExtendedNodes, ReachedEnd};
use scoring::AlignmentStateTree;


pub struct PoastaAligner<'a, C>
where
    C: AlignmentCosts
{
    costs: C,
    debug_output: Option<&'a DebugOutputWriter>,
}

impl<'a, C> PoastaAligner<'a, C>
where
    C: AlignmentCosts
{
    pub fn new(costs: C) -> Self {
        Self {
            costs,
            debug_output: None,
        }
    }

    pub fn new_with_debug_output(costs: C, debug_writer: &'a DebugOutputWriter) -> Self {
        PoastaAligner {
            costs,
            debug_output: Some(debug_writer),
        }
    }

    pub fn align<O, Ix, G, S, N>(&mut self, graph: &G, sequence: &S) -> (usize, Alignment<N>)
    where
        O: OffsetType,
        Ix: TreeIndexType,
        G: AlignableGraph<NodeIndex=N>,
        S: AsRef<[u8]>,
        N: NodeIndexType,
    {
        let seq = sequence.as_ref();
        let max_offset = O::max_value().as_usize();

        assert!(seq.len() - 1 < max_offset, "Sequence is too long for Offset integer type!");

        let mut queue = AlignStateQueue::new();
        let mut state_tree: <C as AlignmentCosts>::StateTreeType<N, O, Ix> = self.costs.to_new_state_tree(graph);

        // Add graph start nodes to queue
        for start_node in graph.start_nodes().iter() {
            let start_state = StateTreeNode::new_start(*start_node);
            let new_ix = state_tree.add_node(start_state);
            queue.enqueue(0, new_ix);
        }

        let mut score = 0;
        let reached_end_state;
        loop {
            let Some(mut current) = queue.pop_current() else {
                panic!("Empty queue?")
            };

            if current.is_empty() {
                score += 1;
                continue;
            }

            // Close indels for current score, and add to current queue
            let new_states = state_tree.close_indels_for(current.as_ref());
            current.extend(new_states.into_iter());

            // Try to extend the alignment along matching sequence in the graph
            match self.extend(graph, seq, &mut state_tree, &mut current) {
                ReachedEnd(end) => {
                    reached_end_state = end;
                    break;
                },
                NewExtendedNodes(additional_nodes) => current.extend(additional_nodes.into_iter())
            }

            // If the end not reached yet, expand into next alignment states, including mismatches
            // and indels. New states to explore are queued per score, such that lower scores are
            // explored first.
            for state_ix in current {
                state_tree.generate_next(&mut queue, graph, seq.len(), state_ix);
            }

            score += 1;
        }

        let alignment = self.backtrace(&state_tree, reached_end_state);

        if let Some(debug) = self.debug_output {
            debug.log(DebugOutputMessage::new_from_state_tree(&state_tree));
        }

        (score, alignment)
    }

    fn extend<O, Ix, G, N, T>(
        &mut self,
        graph: &G,
        seq: &[u8],
        tree: &mut T,
        queue: &mut [Ix]
    ) -> ExtendResult<Ix>
    where
        O: OffsetType,
        Ix: TreeIndexType,
        G: AlignableGraph<NodeIndex=N>,
        N: NodeIndexType,
        C: AlignmentCosts<StateTreeType<N, O, Ix> = T>,
        T: AlignmentStateTree<N, O, Ix>,
    {
        // Before extending, see if we already reached an end state
        if let Some(end) = queue.iter()
            .find(|ix| {
                let node = tree.get_node(**ix);
                match node.state() {
                    AlignState::Start | AlignState::Match | AlignState::Mismatch => {
                        node.offset().as_usize() == seq.len() && graph.is_end(node.node())
                    },
                    _ => false
                }
            })
        {
            return ReachedEnd(*end);
        }

        // Let's try to extend along matching sequence in the graph
        let mut additional_states = Vec::with_capacity(queue.len());
        for node_ix in queue.iter_mut() {
            let node = tree.get_node(*node_ix);

            match node.state() {
                AlignState::Start | AlignState::Match | AlignState::Mismatch => {
                    let path_extender = PathExtender::new(graph, seq, tree, *node_ix);

                    let mut first = true;
                    for new_tree_ix in path_extender {
                        if first {
                            *node_ix = new_tree_ix;
                            first = false;
                        } else {
                            additional_states.push(new_tree_ix)
                        }
                    }
                },
                AlignState::Deletion | AlignState::Deletion2 | AlignState::Insertion | AlignState::Insertion2 => ()
            }
        }

        // Check if one of our extended paths have reached the end
        if let Some(end) = queue.iter().chain(additional_states.iter())
            .find(|ix| {
                let node = tree.get_node(**ix);
                match node.state() {
                    AlignState::Start | AlignState::Match | AlignState::Mismatch => {
                        node.offset().as_usize() == seq.len() && graph.is_end(node.node())
                    },
                    _ => false
                }
            })
        {
            return ReachedEnd(*end);
        }

        NewExtendedNodes(additional_states)
    }

    fn backtrace<O, Ix, N, T>(
        &self,
        tree: &T,
        end_state: Ix
    ) -> Alignment<N>
    where
        O: OffsetType,
        Ix: TreeIndexType,
        N: NodeIndexType,
        T: AlignmentStateTree<N, O, Ix>,
        C: AlignmentCosts<StateTreeType<N, O, Ix> = T>,
    {
        let mut curr = Some(end_state);
        let mut alignment = Alignment::new();

        while let Some(n) = curr {
            let state = tree.get_node(n);

            let Some(bt) = state.backtrace() else {
                break;
            };

            match state.state() {
                AlignState::Match | AlignState::Mismatch => {
                    match bt {
                        Backtrace::Step(_) => {
                            alignment.push(AlignedPair { rpos: Some(state.node()), qpos: Some(state.offset().as_usize() - 1) });
                        },
                        // On indel close we don't have to do anything, the next iteration will take care of the indel
                        Backtrace::ClosedIndel(_) => (),
                    }
                },
                AlignState::Insertion => {
                    alignment.push(AlignedPair { rpos: None, qpos: Some(state.offset().as_usize() - 1) });
                },
                AlignState::Deletion => {
                    alignment.push(AlignedPair { rpos: Some(state.node()), qpos: None });
                },
                AlignState::Start | AlignState::Insertion2 | AlignState::Deletion2 =>
                    panic!("Unexpected align state in backtrace!")
            }

            let prev_node = bt.prev();
            curr = match tree.get_node(prev_node).state() {
                AlignState::Start => None,
                _ => Some(prev_node)
            }
        }

        alignment.reverse();
        alignment
    }
}
