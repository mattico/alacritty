// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! State management for a selection in the grid
//!
//! A selection should start when the mouse is clicked, and it should be
//! finalized when the button is released. The selection should be cleared
//! when text is added/removed/scrolled on the screen. The selection should
//! also be cleared if the user clicks off of the selection.
use std::mem;
use std::ops::RangeInclusive;

use index::{Point, Column, Side, Linear, Line};
use grid::ToRange;

/// The area selected
///
/// Contains all the logic for processing mouse position events and providing
/// necessary info the the renderer.
#[derive(Debug)]
pub enum Selection {
    /// No current selection or start of a selection
    Empty,

    Active {
        start: Point,
        end: Point,
        start_side: Side,
        end_side: Side
    },
}

impl Default for Selection {
    fn default() -> Selection {
        Selection::Empty
    }
}

impl Selection {
    /// Create a selection in the default state
    #[inline]
    pub fn new() -> Selection {
        Default::default()
    }

    /// Clear the active selection
    pub fn clear(&mut self) {
        mem::replace(self, Selection::Empty);
    }

    pub fn is_empty(&self) -> bool {
        match *self {
            Selection::Empty => true,
            _ => false
        }
    }

    pub fn update(&mut self, location: Point, side: Side) {
        let selection = mem::replace(self, Selection::Empty);
        let selection = match selection {
            Selection::Empty => {
                // Start a selection
                Selection::Active {
                    start: location,
                    end: location,
                    start_side: side,
                    end_side: side
                }
            },
            Selection::Active { start, start_side, .. } => {
                // Update ends
                Selection::Active {
                    start: start,
                    start_side: start_side,
                    end: location,
                    end_side: side
                }
            }
        };

        mem::replace(self, selection);
    }

    pub fn span(&self) -> Option<Span> {
        match *self {
            Selection::Active { ref start, ref end, ref start_side, ref end_side } => {
                let (front, tail, front_side, tail_side) = if *start > *end {
                    // Selected upward; start/end are swapped
                    (end, start, end_side, start_side)
                } else {
                    // Selected downward; no swapping
                    (start, end, start_side, end_side)
                };

                debug_assert!(!(tail < front));

                // Single-cell selections are a special case
                if start == end {
                    if start_side != end_side {
                        return Some(Span {
                            ty: SpanType::Inclusive,
                            front: *front,
                            tail: *tail
                        });
                    } else {
                        return None;
                    }
                }

                // The other special case is two adjacent cells with no
                // selection: [ B][E ] or [ E][B ]
                let adjacent = tail.line == front.line && tail.col - front.col == Column(1);
                if adjacent && *front_side == Side::Right && *tail_side == Side::Left {
                    return None;
                }

                Some(match (*front_side, *tail_side) {
                    // [FX][XX][XT]
                    (Side::Left, Side::Right) => Span {
                        front: *front,
                        tail: *tail,
                        ty: SpanType::Inclusive
                    },
                    // [ F][XX][T ]
                    (Side::Right, Side::Left) => Span {
                        front: *front,
                        tail: *tail,
                        ty: SpanType::Exclusive
                    },
                    // [FX][XX][T ]
                    (Side::Left, Side::Left) => Span {
                        front: *front,
                        tail: *tail,
                        ty: SpanType::ExcludeTail
                    },
                    // [ F][XX][XT]
                    (Side::Right, Side::Right) => Span {
                        front: *front,
                        tail: *tail,
                        ty: SpanType::ExcludeFront
                    },
                })
            },
            Selection::Empty => None
        }
    }
}

/// How to interpret the locations of a Span.
#[derive(Debug, Eq, PartialEq)]
pub enum SpanType {
    /// Includes the beginning and end locations
    Inclusive,

    /// Exclude both beginning and end
    Exclusive,

    /// Excludes last cell of selection
    ExcludeTail,

    /// Excludes first cell of selection
    ExcludeFront,
}

/// Represents a span of selected cells
#[derive(Debug, Eq, PartialEq)]
pub struct Span {
    front: Point,
    tail: Point,

    /// The type says whether ends are included or not.
    ty: SpanType,
}

impl Span {
    pub fn to_locations(&self, cols: Column) -> (Point, Point) {
        match self.ty {
            SpanType::Inclusive => (self.front, self.tail),
            SpanType::Exclusive => {
                (Span::wrap_start(self.front, cols), Span::wrap_end(self.tail, cols))
            },
            SpanType::ExcludeFront => (Span::wrap_start(self.front, cols), self.tail),
            SpanType::ExcludeTail => (self.front, Span::wrap_end(self.tail, cols))
        }
    }

    fn wrap_start(mut start: Point, cols: Column) -> Point {
        if start.col == cols - 1 {
            Point {
                line: start.line + 1,
                col: Column(0),
            }
        } else {
            start.col += 1;
            start
        }
    }

    fn wrap_end(end: Point, cols: Column) -> Point {
        if end.col == Column(0) && end.line != Line(0) {
            Point {
                line: end.line - 1,
                col: cols
            }
        } else {
            Point {
                line: end.line,
                col: end.col - 1
            }
        }
    }

    #[inline]
    fn exclude_start(start: Linear) -> Linear {
        start + 1
    }

    #[inline]
    fn exclude_end(end: Linear) -> Linear {
        if end > Linear(0) {
            end - 1
        } else {
            end
        }
    }
}

impl ToRange for Span {
    fn to_range(&self, cols: Column) -> RangeInclusive<Linear> {
        let start = Linear(self.front.line.0 * cols.0 + self.front.col.0);
        let end = Linear(self.tail.line.0 * cols.0 + self.tail.col.0);

        let (start, end) = match self.ty {
            SpanType::Inclusive => (start, end),
            SpanType::Exclusive => (Span::exclude_start(start), Span::exclude_end(end)),
            SpanType::ExcludeFront => (Span::exclude_start(start), end),
            SpanType::ExcludeTail => (start, Span::exclude_end(end))
        };

        start...end
    }
}

/// Tests for selection
///
/// There are comments on all of the tests describing the selection. Pictograms
/// are used to avoid ambiguity. Grid cells are represented by a [  ]. Only
/// cells that are comletely covered are counted in a selection. Ends are
/// represented by `B` and `E` for begin and end, respectively.  A selected cell
/// looks like [XX], [BX] (at the start), [XB] (at the end), [XE] (at the end),
/// and [EX] (at the start), or [BE] for a single cell. Partially selected cells
/// look like [ B] and [E ].
#[cfg(test)]
mod test {
    use index::{Line, Column, Side, Point};
    use super::{Selection, Span, SpanType};

    /// Test case of single cell selection
    ///
    /// 1. [  ]
    /// 2. [B ]
    /// 3. [BE]
    #[test]
    fn single_cell_left_to_right() {
        let location = Point { line: Line(0), col: Column(0) };
        let mut selection = Selection::Empty;
        selection.update(location, Side::Left);
        selection.update(location, Side::Right);

        assert_eq!(selection.span().unwrap(), Span {
            ty: SpanType::Inclusive,
            front: location,
            tail: location
        });
    }

    /// Test case of single cell selection
    ///
    /// 1. [  ]
    /// 2. [ B]
    /// 3. [EB]
    #[test]
    fn single_cell_right_to_left() {
        let location = Point { line: Line(0), col: Column(0) };
        let mut selection = Selection::Empty;
        selection.update(location, Side::Right);
        selection.update(location, Side::Left);

        assert_eq!(selection.span().unwrap(), Span {
            ty: SpanType::Inclusive,
            front: location,
            tail: location
        });
    }

    /// Test adjacent cell selection from left to right
    ///
    /// 1. [  ][  ]
    /// 2. [ B][  ]
    /// 3. [ B][E ]
    #[test]
    fn between_adjacent_cells_left_to_right() {
        let mut selection = Selection::Empty;
        selection.update(Point::new(Line(0), Column(0)), Side::Right);
        selection.update(Point::new(Line(0), Column(1)), Side::Left);

        assert_eq!(selection.span(), None);
    }

    /// Test adjacent cell selection from right to left
    ///
    /// 1. [  ][  ]
    /// 2. [  ][B ]
    /// 3. [ E][B ]
    #[test]
    fn between_adjacent_cells_right_to_left() {
        let mut selection = Selection::Empty;
        selection.update(Point::new(Line(0), Column(1)), Side::Left);
        selection.update(Point::new(Line(0), Column(0)), Side::Right);

        assert_eq!(selection.span(), None);
    }

    /// Test selection across adjacent lines
    ///
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][  ][  ][  ][  ]
    ///     [  ][ B][  ][  ][  ]
    /// 3.  [  ][ E][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    #[test]
    fn across_adjacent_lines_upward_final_cell_exclusive() {
        let mut selection = Selection::Empty;
        selection.update(Point::new(Line(1), Column(1)), Side::Right);
        selection.update(Point::new(Line(0), Column(1)), Side::Right);

        assert_eq!(selection.span().unwrap(), Span {
            front: Point::new(Line(0), Column(1)),
            tail: Point::new(Line(1), Column(1)),
            ty: SpanType::ExcludeFront
        });
    }

    /// Test selection across adjacent lines
    ///
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][ B][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 3.  [  ][ B][XX][XX][XX]
    ///     [XX][XE][  ][  ][  ]
    /// 4.  [  ][ B][XX][XX][XX]
    ///     [XE][  ][  ][  ][  ]
    #[test]
    fn selection_bigger_then_smaller() {
        let mut selection = Selection::Empty;
        selection.update(Point::new(Line(0), Column(1)), Side::Right);
        selection.update(Point::new(Line(1), Column(1)), Side::Right);
        selection.update(Point::new(Line(1), Column(0)), Side::Right);

        assert_eq!(selection.span().unwrap(), Span {
            front: Point::new(Line(0), Column(1)),
            tail: Point::new(Line(1), Column(0)),
            ty: SpanType::ExcludeFront
        });
    }
}
