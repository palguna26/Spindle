use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    Pane {
        pane_id: String,
    },
    Split {
        direction: Direction,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn pane(pane_id: impl Into<String>) -> Self {
        Self::Pane {
            pane_id: pane_id.into(),
        }
    }

    pub fn split(self, direction: Direction, ratio: f32, new_pane_id: impl Into<String>) -> Self {
        Self::Split {
            direction,
            ratio: clamp_ratio(ratio),
            first: Box::new(self),
            second: Box::new(Self::pane(new_pane_id)),
        }
    }

    pub fn split_pane(
        self,
        target_pane_id: &str,
        direction: Direction,
        new_pane_id: impl Into<String>,
    ) -> Option<Self> {
        let new_pane_id = new_pane_id.into();
        match self {
            Self::Pane { pane_id } if pane_id == target_pane_id => {
                Some(Self::Pane { pane_id }.split(direction, 0.5, new_pane_id))
            }
            Self::Pane { pane_id } => Some(Self::Pane { pane_id }),
            Self::Split {
                direction: current_direction,
                ratio,
                first,
                second,
            } => {
                if first.contains(target_pane_id) {
                    Some(Self::Split {
                        direction: current_direction,
                        ratio,
                        first: Box::new(first.split_pane(
                            target_pane_id,
                            direction,
                            new_pane_id,
                        )?),
                        second,
                    })
                } else if second.contains(target_pane_id) {
                    Some(Self::Split {
                        direction: current_direction,
                        ratio,
                        first,
                        second: Box::new(second.split_pane(
                            target_pane_id,
                            direction,
                            new_pane_id,
                        )?),
                    })
                } else {
                    Some(Self::Split {
                        direction: current_direction,
                        ratio,
                        first,
                        second,
                    })
                }
            }
        }
    }

    pub fn resize_pane(&mut self, pane_id: &str, delta: f32) -> bool {
        match self {
            Self::Pane { .. } => false,
            Self::Split {
                ratio,
                first,
                second,
                ..
            } => {
                if first.is_pane(pane_id) {
                    *ratio = clamp_ratio(*ratio + delta);
                    true
                } else if second.is_pane(pane_id) {
                    *ratio = clamp_ratio(*ratio - delta);
                    true
                } else {
                    first.resize_pane(pane_id, delta) || second.resize_pane(pane_id, delta)
                }
            }
        }
    }

    pub fn next_pane<'a>(&'a self, current_pane_id: Option<&str>) -> Option<&'a str> {
        let ids = self.pane_ids();
        if ids.is_empty() {
            return None;
        }
        let index = current_pane_id
            .and_then(|current| ids.iter().position(|pane| *pane == current))
            .map(|index| (index + 1) % ids.len())
            .unwrap_or(0);
        Some(ids[index])
    }

    pub fn pane_ids(&self) -> Vec<&str> {
        match self {
            Self::Pane { pane_id } => vec![pane_id.as_str()],
            Self::Split { first, second, .. } => {
                let mut ids = first.pane_ids();
                ids.extend(second.pane_ids());
                ids
            }
        }
    }

    pub fn close_pane(self, pane_id: &str) -> Option<Self> {
        match self {
            Self::Pane { pane_id: current } => {
                (current != pane_id).then_some(Self::Pane { pane_id: current })
            }
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => match (*first, *second) {
                (first, second) if first.contains(pane_id) => {
                    let collapse = matches!(first, LayoutNode::Pane { .. });
                    if collapse {
                        Some(second)
                    } else {
                        first.close_pane(pane_id).map(|remaining| Self::Split {
                            direction,
                            ratio,
                            first: Box::new(remaining),
                            second: Box::new(second),
                        })
                    }
                }
                (first, second) if second.contains(pane_id) => {
                    let collapse = matches!(second, LayoutNode::Pane { .. });
                    if collapse {
                        Some(first)
                    } else {
                        second.close_pane(pane_id).map(|remaining| Self::Split {
                            direction,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(remaining),
                        })
                    }
                }
                (first, second) => Some(Self::Split {
                    direction,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
            },
        }
    }

    fn contains(&self, pane_id: &str) -> bool {
        self.pane_ids().contains(&pane_id)
    }

    fn is_pane(&self, pane_id: &str) -> bool {
        matches!(self, Self::Pane { pane_id: current } if current == pane_id)
    }
}

pub fn clamp_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.1, 0.9)
}

#[cfg(test)]
mod tests {
    use super::{Direction, LayoutNode};

    #[test]
    fn split_clamps_ratio_and_preserves_order() {
        let layout = LayoutNode::pane("one").split(Direction::Vertical, 2.0, "two");
        assert_eq!(layout.pane_ids(), vec!["one", "two"]);
        assert!(matches!(layout, LayoutNode::Split { ratio, .. } if ratio == 0.9));
    }

    #[test]
    fn closing_a_leaf_collapses_its_parent() {
        let layout = LayoutNode::pane("one").split(Direction::Horizontal, 0.5, "two");
        assert_eq!(
            layout.clone().close_pane("one"),
            Some(LayoutNode::pane("two"))
        );
        assert_eq!(layout.close_pane("two"), Some(LayoutNode::pane("one")));
    }

    #[test]
    fn closing_an_unknown_pane_keeps_layout() {
        let layout = LayoutNode::pane("one").split(Direction::Horizontal, 0.5, "two");
        assert_eq!(layout.clone().close_pane("missing"), Some(layout));
    }

    #[test]
    fn split_targets_the_requested_leaf() {
        let layout = LayoutNode::pane("one")
            .split(Direction::Horizontal, 0.5, "two")
            .split_pane("one", Direction::Vertical, "three")
            .unwrap();
        assert_eq!(layout.pane_ids(), vec!["one", "three", "two"]);
    }

    #[test]
    fn resize_uses_the_nearest_parent_split() {
        let mut layout = LayoutNode::pane("one")
            .split(Direction::Horizontal, 0.5, "two")
            .split_pane("two", Direction::Vertical, "three")
            .unwrap();
        assert!(layout.resize_pane("three", 0.1));
        assert!(matches!(
            layout,
            LayoutNode::Split { ref second, .. }
                if matches!(**second, LayoutNode::Split { ratio, .. } if ratio == 0.4)
        ));
    }

    #[test]
    fn focus_cycles_in_pane_order() {
        let layout = LayoutNode::pane("one").split(Direction::Horizontal, 0.5, "two");
        assert_eq!(layout.next_pane(Some("one")), Some("two"));
        assert_eq!(layout.next_pane(Some("two")), Some("one"));
    }
}
