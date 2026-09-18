//! Shared GPUI ScrollHandles for horizontal panes. No JS scroll synchronization.
use gpui::ScrollHandle;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ScrollGroups {
    groups: HashMap<String, ScrollHandle>,
    members: HashMap<u64, String>,
}
impl ScrollGroups {
    pub(crate) fn resolve(
        &mut self,
        id: u64,
        group: Option<&str>,
        handles: &mut HashMap<u64, ScrollHandle>,
    ) -> ScrollHandle {
        let group = group.filter(|name| !name.is_empty());
        if self.members.get(&id).map(String::as_str) != group {
            handles.remove(&id);
        }
        let handle = if let Some(name) = group {
            self.members.insert(id, name.to_owned());
            self.groups.entry(name.to_owned()).or_default().clone()
        } else {
            self.members.remove(&id);
            handles.entry(id).or_default().clone()
        };
        handles.insert(id, handle.clone());
        handle
    }
    pub(crate) fn remove(&mut self, id: u64) {
        self.members.remove(&id);
    }
    pub(crate) fn prune(&mut self, mut live: impl FnMut(u64, &str) -> bool) {
        self.members.retain(|id, group| live(*id, group));
        self.groups
            .retain(|group, _| self.members.values().any(|member| member == group));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};
    #[test]
    fn members_share_native_offsets_and_detach_without_mutating_the_group() {
        let mut groups = ScrollGroups::default();
        let mut handles = HashMap::new();
        let a = groups.resolve(1, Some("table-a"), &mut handles);
        let b = groups.resolve(2, Some("table-a"), &mut handles);
        let other = groups.resolve(3, Some("table-b"), &mut handles);
        a.set_offset(point(px(-80.), px(0.)));
        assert_eq!(b.offset().x, px(-80.));
        assert_eq!(other.offset().x, px(0.));
        let detached = groups.resolve(2, None, &mut handles);
        detached.set_offset(point(px(-12.), px(0.)));
        assert_eq!(a.offset().x, px(-80.));
        assert_eq!(detached.offset().x, px(-12.));
        groups.prune(|id, _| id == 3);
        assert_eq!(groups.groups.len(), 1);
    }
}
