//! Consuming box headers and removing owned cells without rebuilding list links.
use super::{BoxData, DetachedNode, Kind, List, NodeId, NodeStore};
impl NodeStore {
    pub fn take_box(&mut self, node: DetachedNode) -> BoxData {
        let node = self.free(node.0);
        debug_assert!(node.next.is_null());
        let Kind::Box(id) = node.kind else {
            panic!("not a box")
        };
        self.boxes.take(id)
    }
    /// Remove the successor of a list member, or its head when previous is null.
    pub fn remove_after(&mut self, list: &mut List, previous: NodeId) -> Option<DetachedNode> {
        if previous.is_null() {
            return self.pop_front(list);
        }
        let id = self.get(previous).next;
        if id.is_null() {
            return None;
        }
        let next = self.get(id).next;
        self.get_mut(previous).next = next;
        self.get_mut(id).next = NodeId::NULL;
        if list.tail == id {
            list.tail = previous;
        }
        Some(DetachedNode(id))
    }
    pub fn append(&mut self, list: &mut List, node: DetachedNode) {
        debug_assert!(self.get(node.0).next.is_null(), "append detached node");
        self.splice(list, node.into_list());
    }
    pub fn splice(&mut self, destination: &mut List, source: List) {
        if source.is_empty() {
            return;
        }
        if destination.is_empty() {
            destination.head = source.head;
        } else {
            self.get_mut(destination.tail).next = source.head;
        }
        destination.tail = source.tail;
    }
    pub fn pop_front(&mut self, list: &mut List) -> Option<DetachedNode> {
        if list.is_empty() {
            return None;
        }
        let id = list.head;
        list.head = self.get(id).next;
        self.get_mut(id).next = NodeId::NULL;
        if list.head.is_null() {
            list.tail = NodeId::NULL;
        }
        Some(DetachedNode(id))
    }
    /// Split after a node belonging to this chain; null splits before the head.
    pub fn split_after(&mut self, list: &mut List, node: NodeId) -> List {
        if node.is_null() {
            return std::mem::take(list);
        }
        let head = self.get(node).next;
        if head.is_null() {
            return List::default();
        }
        self.get_mut(node).next = NodeId::NULL;
        let suffix = List::from_parts(head, list.tail);
        list.tail = node;
        suffix
    }
}
