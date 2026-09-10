#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq)]
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
                (current != pane_id).then(|| Self::Pane { pane_id: current })
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
}
