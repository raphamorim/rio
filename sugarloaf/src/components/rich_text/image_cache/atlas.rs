/// Shelf style dynamic atlas allocator.
pub struct AtlasAllocator {
    width: u16,
    height: u16,
    y: u16,
    lines: Vec<Line>,
    slots: Vec<Slot>,
    free_slot: u32,
}

impl AtlasAllocator {
    /// Creates a new atlas with the specified dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            y: 0,
            lines: Vec::new(),
            slots: Vec::new(),
            free_slot: !0,
        }
    }

    /// Allocates a rectangle in the atlas if possible. Returns the x and y
    /// coordinates of the allocated slot.
    pub fn allocate(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        // Width is a hard constraint; make sure it fits.
        if width > self.width {
            return None;
        }
        // Find a current line that supports the image height with some slop.
        let padded_height = height.checked_add(1)?;
        let padded_width = width.checked_add(1)?;
        let mut best_line = None;
        for (i, line) in self.lines.iter().enumerate() {
            if line.height >= padded_height {
                if let Some((_, _, best_height)) = best_line {
                    if line.height < best_height {
                        if let Some(x) = self.check_width(line, padded_width) {
                            best_line = Some((i, x, line.height));
                        }
                    }
                } else if let Some(x) = self.check_width(line, padded_width) {
                    best_line = Some((i, x, line.height));
                }
            }
        }
        let (line_index, slot) = match best_line {
            Some((line_index, slot, line_height)) => {
                if line_height > (padded_height + padded_height / 2) {
                    if let Some(line_index) = self.allocate_line(padded_height) {
                        (line_index, FreeSlot::Direct(0))
                    } else {
                        (line_index, slot)
                    }
                } else {
                    (line_index, slot)
                }
            }
            None => {
                let line_index = self.allocate_line(padded_height)?;
                (line_index, FreeSlot::Direct(0))
            }
        };
        let line = self.lines.get_mut(line_index)?;
        let y = if line.y == 0 { 0 } else { line.y + 1 };
        match slot {
            FreeSlot::Direct(x) => {
                line.state = (x as u32 + padded_width as u32).min(self.width as u32);
                Some((x, y))
            }
            FreeSlot::Node(prev_index, slot_index) => {
                let slot = self.slots.get_mut(slot_index as usize)?;
                let x = slot.x;
                let end = (x as u32 + padded_width as u32).min(self.width as u32);
                let slot_end = slot.end();
                if end != slot_end {
                    slot.x = end as u16;
                    slot.width = (slot_end - end) as u16;
                } else {
                    let slot = *slot;
                    if let Some(prev) = prev_index {
                        self.slots[prev as usize].next = slot.next;
                    } else if slot.next == !0 {
                        // We're filling the last slot with no previous
                        // slot. Revert to the offset state.
                        self.lines[line_index].state = slot_end;
                    } else {
                        self.lines[line_index].state = FRAGMENTED_BIT | slot.next;
                    }
                    self.free_slot(slot_index);
                }
                Some((x, y))
            }
        }
    }

    /// Deallocates the slot with the specified coordinates and width.
    #[allow(unused)]
    pub fn deallocate(&mut self, x: u16, y: u16, width: u16) -> bool {
        let res = self.deallocate_impl(x, y, width).is_some();
        while self.lines.last().map(|l| l.state) == Some(0) {
            let line = self.lines.pop().unwrap();
            self.y = line.y;
        }
        res
    }

    #[allow(unused)]
    fn deallocate_impl(&mut self, x: u16, y: u16, width: u16) -> Option<()> {
        let (line_index, &line) = if y == 0 {
            self.lines
                .iter()
                .enumerate()
                .find(|(_, line)| line.y == y)?
        } else {
            let y = y - 1;
            self.lines
                .iter()
                .enumerate()
                .find(|(_, line)| line.y == y)?
        };
        let end = (x as u32 + width as u32 + 1).min(self.width as u32);
        let actual_width = (end - x as u32) as u16;
        if line.state & FRAGMENTED_BIT == 0 {
            let offset = line.state & LOW_BITS;
            // If it was the last allocation, just fold it in.
            if offset == end {
                self.lines[line_index].state = x as u32;
                return Some(());
            }
            // Otherwise, add a new free slot for the evicted rect and an
            // additional slot for the remaining space if any.
            let slot_index = self.allocate_slot(x, actual_width)?;
            let remaining = self.width - offset as u16;
            if remaining != 0 {
                let remaining_slot_index =
                    self.allocate_slot(offset as u16, remaining)?;
                self.slots[slot_index as usize].next = remaining_slot_index;
            }
            self.lines[line_index].state = FRAGMENTED_BIT | slot_index;
            return Some(());
        } else {
            let mut first_index = line.state & LOW_BITS;
            let mut next_index = first_index;
            // Insert the slot into the sorted list
            let mut prev_index = None;
            while next_index != !0 {
                let slot = self.slots.get(next_index as usize)?;
                if slot.x > x {
                    break;
                }
                prev_index = Some(next_index);
                next_index = slot.next;
            }
            match prev_index {
                Some(prev_index) => {
                    let merge_prev = self.slots[prev_index as usize].end() == x as u32;
                    let merge_next = next_index != !0
                        && self.slots[next_index as usize].x == end as u16;
                    match (merge_prev, merge_next) {
                        (true, true) => {
                            let next = self.slots[next_index as usize];
                            let prev = &mut self.slots[prev_index as usize];
                            prev.width = next.end() as u16 - prev.x;
                            prev.next = next.next;
                            self.free_slot(next_index);
                        }
                        (true, false) => {
                            let prev = &mut self.slots[prev_index as usize];
                            prev.width += actual_width;
                        }
                        (false, true) => {
                            let next = &mut self.slots[next_index as usize];
                            next.width = (next.end() - x as u32) as u16;
                            next.x = x;
                        }
                        (false, false) => {
                            let slot_index = self.allocate_slot(x, end as u16 - x)?;
                            self.slots[slot_index as usize].next = next_index;
                            self.slots[prev_index as usize].next = slot_index;
                        }
                    }
                }
                None => {
                    let next = &mut self.slots[first_index as usize];
                    if next.x == end as u16 {
                        next.width = (next.end() - x as u32) as u16;
                        next.x = x;
                    } else {
                        let slot_index = self.allocate_slot(x, end as u16 - x)?;
                        self.slots[slot_index as usize].next = first_index;
                        self.lines[line_index].state = FRAGMENTED_BIT | slot_index;
                        first_index = slot_index;
                    }
                }
            }
            let first = self.slots[first_index as usize];
            if first.next == !0 && first.end() == self.width as u32 {
                self.free_slot(first_index);
                self.lines[line_index].state = first.x as u32;
            }
        }
        Some(())
    }

    fn allocate_line(&mut self, padded_height: u16) -> Option<usize> {
        let bottom = self.y.checked_add(padded_height)?;
        if bottom > self.height {
            return None;
        }
        let line_index = self.lines.len();
        self.lines.push(Line {
            y: self.y,
            height: padded_height,
            state: 0,
        });
        self.y = bottom;
        Some(line_index)
    }

    #[allow(unused)]
    fn allocate_slot(&mut self, x: u16, width: u16) -> Option<u32> {
        let slot = Slot { x, width, next: !0 };
        if self.free_slot != !0 {
            let index = self.free_slot as usize;
            self.free_slot = self.slots[index].next;
            self.slots[index] = slot;
            Some(index as u32)
        } else {
            let index = u32::try_from(self.slots.len()).ok()?;
            self.slots.push(slot);
            Some(index)
        }
    }

    fn free_slot(&mut self, index: u32) {
        self.slots[index as usize].next = self.free_slot;
        self.free_slot = index;
    }

    fn check_width(&self, line: &Line, width: u16) -> Option<FreeSlot> {
        if line.state & FRAGMENTED_BIT == 0 {
            let x = (line.state & LOW_BITS) as u16;
            let end = x.checked_add(width - 1)?;
            if end <= self.width {
                return Some(FreeSlot::Direct(x));
            }
        } else {
            let mut cur_slot_index = line.state & LOW_BITS;
            let mut prev_slot_index = None;
            let mut best_slot = None;
            while cur_slot_index != !0 {
                let slot = self.slots.get(cur_slot_index as usize)?;
                if slot.width >= width
                    || (slot.end() == self.width as u32 && slot.width >= (width - 1))
                {
                    match best_slot {
                        Some((_, _, best_width)) => {
                            if slot.width < best_width {
                                best_slot =
                                    Some((prev_slot_index, cur_slot_index, slot.width));
                            }
                        }
                        None => {
                            best_slot =
                                Some((prev_slot_index, cur_slot_index, slot.width));
                        }
                    }
                }
                prev_slot_index = Some(cur_slot_index);
                cur_slot_index = slot.next;
            }
            let (prev, index, _) = best_slot?;
            return Some(FreeSlot::Node(prev, index));
        }
        None
    }

    // pub fn dump_lines(&self) {
    //     for (i, line) in self.lines.iter().enumerate() {
    //         print!("[{}]", i);
    //         let low_bits = line.state & LOW_BITS;
    //         if line.state & FRAGMENTED_BIT == 0 {
    //             println!(" offset {}", low_bits);
    //         } else {
    //             let mut itr = low_bits;
    //             while itr != !0 {
    //                 let slot = self.slots[itr as usize];
    //                 print!(" ({}..={})", slot.x, slot.x + slot.width);
    //                 itr = slot.next;
    //             }
    //             println!("");
    //         }
    //     }
    // }
}

const FRAGMENTED_BIT: u32 = 0x80000000;
const LOW_BITS: u32 = !FRAGMENTED_BIT;

#[derive(Copy, Clone, Default, Debug)]
struct Line {
    y: u16,
    height: u16,
    /// This field encodes the current state of the line. If there
    /// have been no evictions, then the high bit will be clear and
    /// the remaining bits will contain the x-offset of the next
    /// available region (bump pointer allocation). If the high bit
    /// is set, then the low bits are an index into the free node
    /// list.    
    state: u32,
}

enum FreeSlot {
    Direct(u16),
    Node(Option<u32>, u32),
}

#[derive(Copy, Clone, Default)]
struct Slot {
    x: u16,
    width: u16,
    next: u32,
}

impl Slot {
    fn end(self) -> u32 {
        self.x as u32 + self.width as u32
    }
}
