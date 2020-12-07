use crate::show_db::LARGE_NUMBER;
use isnt::std_1::ops::IsntRangeExt;
use num_traits::ToPrimitive;
use std::{cell::Cell, ops::Range};

/// A data structure for efficient prefix search
///
/// This data structure is an array of nodes. Each node contains a range of elements
/// higher up (with higher indices) in the array that are the children of the node.
/// Each node has exactly one parent (except for the root node).
///
/// Each node (except for the root node) has an associated ascii letter `[a-z0-9]`.
/// By walking the tree from the root, one can therefore construct an ascii string
/// `[a-z0-9]+`. For each node, the thus constructed string is the key of the node. Given
/// this key and the usual ascii string ordering, this data structure is a min heap.
///
/// Furthermore, no two children of a node have the same associated letter. Therefore,
/// each node has a unique associated key.
///
/// Given an ascii string `[a-z0-9]*`, one can therefore easily find the node in the heap
/// whose key is the longest prefix of given string in the heap.
///
/// Each node in the heap has an associated (possibly empty) array of payloads of type
/// `T`.
pub struct AsciiHeap<T> {
    payloads: Box<[T]>,
    nodes: Box<[Node]>,
}

struct Node {
    letter: u8,
    // each node can have at most as many children as there are letters in [a-z0-9]
    num_children: u8,
    pos_children: u32,
    payloads: Range<u32>,
}

impl Node {
    fn children(&self) -> Range<usize> {
        let pos = self.pos_children as usize;
        let num = self.num_children as usize;
        pos..pos + num
    }

    fn payloads(&self) -> Range<usize> {
        self.payloads.start as usize..self.payloads.end as usize
    }
}

impl<T> AsciiHeap<T> {
    /// Creates a new heap from the given iterator
    ///
    /// The first component of the iterator will first be converted to ascii lowercase and
    /// all `[a-z0-9]` letters will be removed. The resulting string serves as the key.
    /// The node in the heap with this key will have the associated payload of the second
    /// component. If the same key occurs multiple times, then all of the payloads will be
    /// associated with that node.
    ///
    /// Note that if the resulting string is empty, the payload will not be contained in
    /// the heap.
    pub fn new<'a, I: IntoIterator<Item = (&'a str, T)>>(strs: I) -> Self {
        // Step 1: For each prefix of each element in the iterator, create a PreData
        // object. The PreData object of the full string of an element has the payload
        // associated with it. Most of these (except duplicates) will later be turned
        // into nodes.
        struct PreData<T> {
            substring_range: Range<usize>,
            payload: Option<T>,
        }

        // lc contains the concatenation of all strings in the iterator
        // (after ascii reduction).
        let mut lc = Vec::with_capacity(10 * LARGE_NUMBER);
        let mut pre_datas = Vec::with_capacity(10 * LARGE_NUMBER);
        let mut num_payloads = 0;

        for (s, payload) in strs {
            let start = lc.len();
            for &c in s.as_bytes().iter() {
                let c = c.to_ascii_lowercase();
                if matches!(c, b'a'..=b'z' | b'0'..=b'9') {
                    lc.push(c);
                    pre_datas.push(PreData {
                        substring_range: start..lc.len(),
                        payload: None,
                    });
                }
            }
            if start < lc.len() {
                pre_datas.last_mut().unwrap().payload = Some(payload);
                num_payloads += 1;
            }
        }

        assert!(
            pre_datas.len().to_u32().is_some(),
            "AsciiHeap supports at most u32::MAX nodes"
        );

        // Step 2: Sort the PreData by the ascii order of their associated strings. This
        // ensures that the array has the following property: All of the descendants of a
        // node occur in an array immediately after the node. This allows us to find the
        // children of a node using a simple stack algorithm.
        pre_datas.sort_by_key(|data| &lc[data.substring_range.clone()]);

        // Step 3: For each PreData object that is not a duplicate (duplicate meaning that
        // is has the same associated key) create a Data object. After this, each data
        // object will contain the following information:
        // - how many children it has
        // - the position of its parent in the array
        // - the position of its payloads in the payload array
        struct Data {
            letter: u8,
            payload_range: Range<u32>,
            num_children: u8,
            parent: Option<u32>,
            heap_pos: Cell<u32>,
            children_heap_pos: Cell<u32>,
            next_child_pos: Cell<u32>,
        }

        let mut datas: Vec<Data> = Vec::with_capacity(LARGE_NUMBER);
        let mut payloads = Vec::with_capacity(num_payloads);
        let mut num_single_letter = 0;

        let mut stack: Vec<u32> = vec![];
        let mut prev_letter = 0;
        let mut prev_len = 0;

        for pre_data in pre_datas {
            // This is the associated letter of the node
            let letter = lc[pre_data.substring_range.end - 1];
            let len = pre_data.substring_range.len();
            // If the length is the same and the last letter is the same as the previous
            // element of the array, then the whole associated string must be the same.
            // This is because the array is prefix-complete. (Meaning that every prefix
            // of a string in the array also occurs in the array.) If two strings A and B
            // are in the array and A occurs before B, then each prefix of B that is not
            // a prefix of A will occur between A and B in the array. Example:
            // A = aaaaa
            //     aab
            // B = aaba
            let is_duplicate = (letter, len) == (prev_letter, prev_len);
            // The payload range is empty unless the element has a payload.
            let mut payload_range = (payloads.len() as u32)..(payloads.len() as u32);
            if let Some(payload) = pre_data.payload {
                payloads.push(payload);
                if is_duplicate {
                    // If it's a duplicate, the payload will be associated with the
                    // previous element.
                    datas.last_mut().unwrap().payload_range.end += 1;
                } else {
                    payload_range.end += 1;
                }
            }
            if is_duplicate {
                // Nothing else to do for duplicates.
                continue;
            }
            // If the previous element has the same length, then it's this element's
            // sibling. If it's 1 longer, then it's our nephew, etc. Pop them off until
            // our parent is at the top of the stack.
            if len <= prev_len {
                for _ in 0..prev_len - len + 1 {
                    assert!(stack.pop().is_some());
                }
            }
            // If there is an element left on the stack, it's our parent. If there is no
            // element left, then our associated string is of length 1. Later on, the root
            // node will become our parent.
            let parent = match stack.last() {
                Some(&idx) => {
                    // Tell our parent that it has one more child.
                    datas[idx as usize].num_children += 1;
                    Some(idx)
                }
                _ => {
                    num_single_letter += 1;
                    None
                }
            };
            prev_letter = letter;
            prev_len = len;
            // Push our index in the array onto the stack.
            stack.push(datas.len() as u32);
            datas.push(Data {
                letter,
                payload_range,
                num_children: 0,
                parent,
                heap_pos: Cell::new(0),
                children_heap_pos: Cell::new(0),
                next_child_pos: Cell::new(0),
            });
        }

        // Step 4: For each element in the array, determine its position in the heap and
        // the position of its children. Note that single letter nodes occur at the start
        // of the array immediately after the root node.
        let mut next_free_node_position = num_single_letter as u32 + 1;
        let mut next_one_letter_pos = 1;

        for data in &datas {
            match data.parent {
                Some(parent_idx) => {
                    // If we have a parent, our position is the next position reserved
                    // for its children.
                    let parent = &datas[parent_idx as usize];
                    let next_child_pos = parent.next_child_pos.get();
                    data.heap_pos.set(next_child_pos);
                    parent.next_child_pos.set(next_child_pos + 1);
                }
                _ => {
                    // If we don't have a parent, our position is at the start of the
                    // array in the single-letter area.
                    data.heap_pos.set(next_one_letter_pos);
                    next_one_letter_pos += 1;
                }
            }
            // Reserve space for our children at the end of the array.
            data.children_heap_pos.set(next_free_node_position);
            data.next_child_pos.set(next_free_node_position);
            next_free_node_position += data.num_children as u32;
        }

        // Step 5: Sort the array by the heap position and transform the entries into
        // nodes.
        datas.sort_by_key(|d| d.heap_pos.get());

        let mut nodes = Vec::with_capacity(datas.len() + 1);
        // This is the root node
        nodes.push(Node {
            letter: 0,
            num_children: num_single_letter,
            pos_children: 1,
            payloads: 0..0,
        });

        for data in datas {
            let children_heap_pos = data.children_heap_pos.get();
            nodes.push(Node {
                letter: data.letter,
                num_children: data.num_children,
                pos_children: children_heap_pos,
                payloads: data.payload_range,
            });
        }

        AsciiHeap {
            payloads: payloads.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
        }
    }

    /// Finds the position of the longest prefix in the heap
    ///
    /// Note that the string should only contain `[a-z0-9]` letters. All other letters
    /// will be ignored, including `[A-Z]`.
    pub fn find(&self, s: &str) -> usize {
        let mut idx = 0;
        let mut children = self.nodes[idx].children();
        for &c in s.as_bytes() {
            if matches!(c, b'a'..=b'z' | b'0'..=b'9') {
                match children
                    .into_iter()
                    .find(|&idx| self.nodes[idx].letter == c)
                {
                    Some(child) => {
                        idx = child;
                        children = self.nodes[idx].children();
                    }
                    _ => break,
                }
            }
        }
        idx
    }

    /// Creates an iterator over all payloads below the node at the index in the heap
    pub fn iter(&self, idx: usize) -> Iter<T> {
        Iter {
            heap: self,
            cur: idx..idx + 1,
            todo: vec![],
            payloads: [].iter(),
        }
    }
}

pub struct Iter<'a, T> {
    heap: &'a AsciiHeap<T>,
    cur: Range<usize>,
    todo: Vec<Range<usize>>,
    payloads: std::slice::Iter<'a, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(next) = self.payloads.next() {
                return Some(next);
            }
            loop {
                if self.cur.is_empty() {
                    self.cur = match self.todo.pop() {
                        Some(r) => r,
                        _ => return None,
                    };
                }
                let node = &self.heap.nodes[self.cur.start];
                self.cur.start += 1;
                if node.num_children > 0 {
                    self.todo.push(node.children());
                }
                if node.payloads.is_not_empty() {
                    self.payloads = self.heap.payloads[node.payloads()].iter();
                    break;
                }
            }
        }
    }
}
