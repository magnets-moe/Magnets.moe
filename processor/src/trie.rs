// use std::{collections::HashMap, slice};
//
// lazy_static::lazy_static! {
//     static ref MAP: [usize; 256] = {
//         let mut res = [0; 256];
//         let mut pos = 0;
//         for i in b'a'..=b'z' {
//             res[i as usize] = pos;
//             pos += 1;
//         }
//         for i in b'0'..=b'9' {
//             res[i as usize] = pos;
//             pos += 1;
//         }
//         res
//     };
// }
//
// const SIZE: usize = (b'z' - b'a' + 1 + 10) as usize;
//
// #[derive(Debug, Clone)]
// pub struct AsciiTrie<T> {
//     child_idx: [u8; SIZE],
//     children: Box<[AsciiTrie<T>]>,
//     data: Box<[T]>,
//     total_nodes: usize,
//     depth: usize,
// }
//
// impl<T> AsciiTrie<T> {
//     pub fn new<'a, I: IntoIterator<Item = (&'a str, T)>>(strs: I) -> Self {
//         #[derive(Default)]
//         struct Node<T> {
//             children: HashMap<u8, Node<T>>,
//             data: Vec<T>,
//         }
//
//         let mut map = HashMap::new();
//         for (s, data) in strs {
//             let mut map = &mut map;
//             let mut datas = None;
//             for &c in s.as_bytes().iter() {
//                 let c = c.to_ascii_lowercase();
//                 if matches!(c, b'a'..=b'z' | b'0'..=b'9') {
//                     let node = map.entry(c).or_insert_with(|| Node {
//                         children: HashMap::new(),
//                         data: vec![],
//                     });
//                     map = &mut node.children;
//                     datas = Some(&mut node.data);
//                 }
//             }
//             if let Some(datas) = datas {
//                 datas.push(data);
//             }
//         }
//
//         let mut stack = vec![];
//         let mut top = (
//             map.into_iter(),
//             [255; SIZE],
//             vec![],
//             vec![].into_boxed_slice(),
//             0,
//             0,
//         );
//         loop {
//             if let Some((c, cn)) = top.0.next() {
//                 top.1[MAP[c as usize]] = top.2.len() as u8;
//                 let depth = top.5;
//                 stack.push(top);
//                 let len = cn.data.len();
//                 top = (
//                     cn.children.into_iter(),
//                     [255; SIZE],
//                     vec![],
//                     cn.data.into_boxed_slice(),
//                     len,
//                     depth + 1,
//                 );
//             } else {
//                 let trie = AsciiTrie {
//                     child_idx: top.1,
//                     children: top.2.into_boxed_slice(),
//                     data: top.3,
//                     total_nodes: top.4,
//                     depth: top.5,
//                 };
//                 if let Some(parent) = stack.pop() {
//                     top = parent;
//                     top.4 += trie.total_nodes;
//                     top.2.push(trie);
//                 } else {
//                     return trie;
//                 }
//             }
//         }
//     }
//
//     pub fn total_nodes(&self) -> usize {
//         self.total_nodes
//     }
//
//     pub fn find(&self, s: &str) -> (&AsciiTrie<T>, &AsciiTrie<T>) {
//         let mut trie = self;
//         let mut full = trie;
//         for &c in s.as_bytes() {
//             if matches!(c, b'a'..=b'z' | b'0'..=b'9') {
//                 let pos = MAP[c as usize];
//                 let idx = trie.child_idx[pos];
//                 if idx < 255 {
//                     trie = &trie.children[idx as usize];
//                     if !trie.data.is_empty() {
//                         full = trie;
//                     }
//                 } else {
//                     break;
//                 }
//             }
//         }
//         (trie, full)
//     }
//
//     pub fn iter(&self) -> impl Iterator<Item = &T> {
//         Iter {
//             ai: self.data.iter(),
//             todo: vec![self.children.iter()],
//         }
//     }
// }
//
// struct Iter<'a, T> {
//     ai: slice::Iter<'a, T>,
//     todo: Vec<slice::Iter<'a, AsciiTrie<T>>>,
// }
//
// impl<'a, T> Iterator for Iter<'a, T> {
//     type Item = &'a T;
//
//     fn next(&mut self) -> Option<Self::Item> {
//         loop {
//             if let Some(i) = self.ai.next() {
//                 return Some(i);
//             }
//             let mut at = match self.todo.pop() {
//                 Some(at) => at,
//                 _ => return None,
//             };
//             if let Some(nt) = at.next() {
//                 self.todo.push(at);
//                 self.todo.push(nt.children.iter());
//                 self.ai = nt.data.iter();
//             }
//         }
//     }
// }
