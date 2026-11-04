//! tmux window layout strings.
//!
//! `a87d,100x30,0,0,0` is one pane; `6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1}`
//! is two side by side. `{}` holds children laid out left to right and `[]`
//! top to bottom. Sizes are in cells and siblings are separated by a
//! one-cell divider.

/// A node of a window's layout, sized in cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutNode {
    pub columns: usize,
    pub lines: usize,
    pub kind: LayoutKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutKind {
    Pane(u32),
    /// Children laid out left to right.
    Row(Vec<LayoutNode>),
    /// Children laid out top to bottom.
    Column(Vec<LayoutNode>),
}

impl LayoutNode {
    /// Parse a layout string, including its leading checksum.
    pub fn parse(layout: &str) -> Option<LayoutNode> {
        let (_checksum, body) = layout.split_once(',')?;
        let mut parser = Parser {
            bytes: body.as_bytes(),
            position: 0,
        };
        let node = parser.node()?;
        (parser.position == parser.bytes.len()).then_some(node)
    }

    /// Every pane in the layout as `(id, columns, lines)`, in visual order.
    pub fn panes(&self) -> Vec<(u32, usize, usize)> {
        let mut panes = Vec::new();
        self.collect_panes(&mut panes);
        panes
    }

    /// A pane through which `resize-pane` moves the divider after this node,
    /// a child of a row if `in_row` and of a column otherwise.
    ///
    /// tmux resizes along the pane's nearest ancestor of that kind, so a pane
    /// nested in another row or column would move a divider of its own.
    pub fn divider_pane(&self, in_row: bool) -> Option<u32> {
        match &self.kind {
            LayoutKind::Pane(id) => Some(*id),
            LayoutKind::Row(_) if in_row => None,
            LayoutKind::Column(_) if !in_row => None,
            LayoutKind::Row(children) | LayoutKind::Column(children) => {
                children.iter().find_map(|c| c.divider_pane(in_row))
            }
        }
    }

    fn collect_panes(&self, panes: &mut Vec<(u32, usize, usize)>) {
        match &self.kind {
            LayoutKind::Pane(id) => panes.push((*id, self.columns, self.lines)),
            LayoutKind::Row(children) | LayoutKind::Column(children) => {
                for child in children {
                    child.collect_panes(panes);
                }
            }
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn node(&mut self) -> Option<LayoutNode> {
        let columns = self.number()?;
        self.expect(b'x')?;
        let lines = self.number()?;
        self.expect(b',')?;
        self.number()?;
        self.expect(b',')?;
        self.number()?;

        let kind = match self.next()? {
            b',' => LayoutKind::Pane(u32::try_from(self.number()?).ok()?),
            b'{' => LayoutKind::Row(self.children(b'}')?),
            b'[' => LayoutKind::Column(self.children(b']')?),
            _ => return None,
        };
        Some(LayoutNode {
            columns,
            lines,
            kind,
        })
    }

    fn children(&mut self, close: u8) -> Option<Vec<LayoutNode>> {
        let mut children = Vec::new();
        loop {
            children.push(self.node()?);
            match self.next()? {
                b',' => (),
                byte if byte == close => return Some(children),
                _ => return None,
            }
        }
    }

    fn number(&mut self) -> Option<usize> {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()?
            .parse()
            .ok()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.position)?;
        self.position += 1;
        Some(byte)
    }

    fn expect(&mut self, byte: u8) -> Option<()> {
        (self.next()? == byte).then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_panes_avoid_nested_nodes_of_the_same_kind() {
        let column = LayoutNode::parse(
            "0000,50x30,0,0[50x15,0,0{25x15,0,0,3,24x15,26,0,4},50x14,0,16,5]",
        )
        .unwrap();
        assert_eq!(column.divider_pane(true), Some(5));
        assert_eq!(column.divider_pane(false), None);

        let rows = LayoutNode::parse(
            "0000,50x30,0,0[50x15,0,0{25x15,0,0,3,24x15,26,0,4},50x14,0,16{25x14,0,16,5,24x14,26,16,6}]",
        )
        .unwrap();
        assert_eq!(rows.divider_pane(true), None);
    }

    #[test]
    fn single_pane() {
        assert_eq!(
            LayoutNode::parse("a87d,100x30,0,0,7"),
            Some(LayoutNode {
                columns: 100,
                lines: 30,
                kind: LayoutKind::Pane(7),
            })
        );
    }

    #[test]
    fn side_by_side_as_emitted_by_tmux() {
        let layout = LayoutNode::parse("6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1}");
        assert_eq!(layout.unwrap().panes(), vec![(0, 50, 30), (1, 49, 30)]);
    }

    #[test]
    fn nested_rows_and_columns() {
        let layout = LayoutNode::parse(
            "1234,80x24,0,0[80x12,0,0,1,80x11,0,13{40x11,0,13,2,39x11,41,13,3}]",
        )
        .unwrap();
        assert_eq!(layout.panes(), vec![(1, 80, 12), (2, 40, 11), (3, 39, 11)]);
        let LayoutKind::Column(children) = &layout.kind else {
            panic!("expected a column");
        };
        assert!(matches!(children[1].kind, LayoutKind::Row(_)));
    }

    #[test]
    fn malformed_layouts_are_rejected() {
        for layout in [
            "",
            "a87d",
            "a87d,100x30,0,0",
            "a87d,100x30,0,0,",
            "a87d,100x30,0,0{50x30,0,0,0",
            "a87d,100x30,0,0{50x30,0,0,0]",
            "a87d,100x30,0,0,7trailing",
        ] {
            assert_eq!(LayoutNode::parse(layout), None, "{layout}");
        }
    }
}
