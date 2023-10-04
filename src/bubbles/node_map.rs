use std::collections::VecDeque;
use rustc_hash::FxHashSet;
use crate::aligner::offsets::OffsetType;
use crate::bubbles::finder::SuperbubbleFinder;
use crate::graphs::{AlignableGraph, NodeIndexType};

#[derive(Copy, Clone)]
enum BubbleNode<N> {
    /// Represents a bubble entrance, the associated data is the corresponding exit node ID
    Entrance(N),

    /// Represents a bubble exit, the associated data is the corresponding entrance node ID
    Exit(N)
}

struct NodeBubbleMapBuilder<'a, O, G>
where
    G: AlignableGraph,
{
    graph: &'a G,

    /// Superbubble finder object
    finder: SuperbubbleFinder<'a, G>,

    /// Array indicating whether a node is a bubble entrance (follows reverse postorder)
    bubble_entrance: Vec<Option<BubbleNode<G::NodeIndex>>>,

    /// Array indicating whether a node is a bubble exit (follows reverse postorder)
    bubble_exit: Vec<Option<BubbleNode<G::NodeIndex>>>,

    /// A list of bubbles containing a particular node
    node_bubble_map: Vec<Vec<NodeBubbleMap<G::NodeIndex, O>>>,

    /// Which nodes have we already processed
    visited: FxHashSet<G::NodeIndex>,
}

impl<'a, O, G> NodeBubbleMapBuilder<'a, O, G>
where
    O: OffsetType,
    G: AlignableGraph,
{
    pub fn new(graph: &'a G) -> Self {
        let finder = SuperbubbleFinder::new(graph);
        let rev_postorder = finder.get_rev_postorder();

        // Two separate lists because nodes can be both an entrance and an exit
        let mut bubble_entrances = vec![None; graph.node_count_with_start()];
        let mut bubble_exits = vec![None; graph.node_count_with_start()];

        for (entrance, exit) in finder.iter() {
            let entrance_order = rev_postorder[entrance.index()];
            let exit_order = rev_postorder[exit.index()];

            bubble_entrances[entrance_order] = Some(BubbleNode::Entrance(exit));
            bubble_exits[exit_order] = Some(BubbleNode::Exit(entrance));
        }

        Self {
            graph,
            finder,
            bubble_entrance: bubble_entrances,
            bubble_exit: bubble_exits,
            node_bubble_map: vec![Vec::default(); graph.node_count_with_start()],
            visited: FxHashSet::default(),
        }
    }

    fn bubble_backward_bfs(&mut self, entrance: G::NodeIndex, exit: G::NodeIndex) {
        let rev_postorder = self.finder.get_rev_postorder();

        // BFS queue, containing the next nodes to explore, with per node the distance from start,
        // along with a stack of active bubbles
        let mut queue: VecDeque<_> = vec![
            (exit, 0usize, vec![(0usize, exit)])
        ].into();
        self.visited.insert(exit);

        while !queue.is_empty() {
            let (curr, dist_from_start, bubble_stack) = queue.pop_front().unwrap();
            let rpo = rev_postorder[curr.index()];

            for (bubble_dist_from_start, bubble_exit) in bubble_stack.iter() {
                self.node_bubble_map[curr.index()].push(NodeBubbleMap {
                    bubble_exit: *bubble_exit,
                    dist_to_exit: O::new(dist_from_start - *bubble_dist_from_start)
                })
            }

            if curr == entrance && self.bubble_exit[rpo].is_none() {
                continue;
            }

            for pred in self.graph.predecessors(curr) {
                if !self.visited.contains(&pred) {
                    let pred_rpo = rev_postorder[pred.index()];
                    let new_dist_from_start = dist_from_start + 1;
                    let mut new_bubble_stack = bubble_stack.clone();

                    if self.bubble_entrance[pred_rpo].is_some() {
                        let (bubble_dist_from_start, bubble_exit) = new_bubble_stack.pop().unwrap();
                        self.node_bubble_map[pred.index()].push(NodeBubbleMap {
                            bubble_exit,
                            dist_to_exit: O::new(new_dist_from_start - bubble_dist_from_start)
                        });
                    }

                    if self.bubble_exit[pred_rpo].is_some() {
                        new_bubble_stack.push((new_dist_from_start, pred));
                    }

                    self.visited.insert(pred);
                    queue.push_back((pred, new_dist_from_start, new_bubble_stack));
                }
            }
        }
    }

    pub fn build(mut self) -> Vec<Vec<NodeBubbleMap<G::NodeIndex, O>>> {
        for rpo in (0..self.graph.node_count_with_start()).rev() {
            let inv_rev_postorder = self.finder.get_inv_rev_postorder();
            let node_id = inv_rev_postorder[rpo];
            if self.visited.contains(&node_id) {
                continue;
            }

            if let Some(bubble_exit_node) = self.bubble_exit[rpo] {
                let BubbleNode::Exit(entrance) = bubble_exit_node else {
                    panic!("Unexpected value for bubble exit!");
                };

                self.bubble_backward_bfs(entrance, inv_rev_postorder[rpo]);
            }
        }

        self.node_bubble_map
    }
}


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct NodeBubbleMap<N, O> {
    bubble_exit: N,
    dist_to_exit: O
}

impl<N, O> NodeBubbleMap<N, O>
where
    N: NodeIndexType,
    O: OffsetType,
{
    pub fn new(bubble_exit: N, dist_to_exit: O) -> Self {
        NodeBubbleMap {
            bubble_exit,
            dist_to_exit
        }
    }
}

#[cfg(test)]
mod tests {
    use petgraph::graph::NodeIndex;
    use crate::bubbles::node_map::NodeBubbleMap;
    use crate::graphs::mock::{create_test_graph1, create_test_graph2};
    use super::NodeBubbleMapBuilder;

    type NIx = NodeIndex<crate::graphs::mock::NIx>;

    #[test]
    pub fn test_bubble_map_builder() {
        let graph1 = create_test_graph1();

        let node_map1 = NodeBubbleMapBuilder::<u32, _>::new(&graph1)
            .build();

        let truth1 = [
            vec![NodeBubbleMap::new(NIx::new(1), 1u32),],
            vec![NodeBubbleMap::new(NIx::new(2), 1u32), NodeBubbleMap::new(NIx::new(1), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(2), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(4), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(5), 1u32), NodeBubbleMap::new(NIx::new(4), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(5), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(8), 1u32), NodeBubbleMap::new(NIx::new(7), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(8), 0u32)]
        ];

        for (n_bubbles, truth) in node_map1.into_iter().zip(truth1.into_iter()) {
            assert_eq!(n_bubbles, truth);
        }

        let graph2 = create_test_graph2();

        let node_map2 = NodeBubbleMapBuilder::<u32, _>::new(&graph2)
            .build();

        let truth2 = [
            vec![NodeBubbleMap::new(NIx::new(2), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(2), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 2u32), NodeBubbleMap::new(NIx::new(2), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(6), 2u32), NodeBubbleMap::new(NIx::new(7), 3u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 2u32), NodeBubbleMap::new(NIx::new(6), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 1u32), NodeBubbleMap::new(NIx::new(6), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(13), 1u32), NodeBubbleMap::new(NIx::new(7), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 3u32), NodeBubbleMap::new(NIx::new(6), 2u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 2u32), NodeBubbleMap::new(NIx::new(6), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(11), 1u32), NodeBubbleMap::new(NIx::new(7), 2u32)],
            vec![NodeBubbleMap::new(NIx::new(7), 1u32), NodeBubbleMap::new(NIx::new(11), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(13), 1u32)],
            vec![NodeBubbleMap::new(NIx::new(13), 0u32)],
            vec![NodeBubbleMap::new(NIx::new(13), 1u32)],
        ];

        for (n_bubbles, truth) in node_map2.into_iter().zip(truth2.into_iter()) {
            assert_eq!(n_bubbles, truth);
        }
    }

}
