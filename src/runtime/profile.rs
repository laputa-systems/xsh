use crate::source::Span;
use rustc_hash::FxHashMap;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SiteKind { Proc, Pure, Module, Method, Core }

impl SiteKind {
    pub fn name(self) -> &'static str {
        match self { Self::Proc => "proc", Self::Pure => "pure", Self::Module => "module", Self::Method => "method", Self::Core => "core" }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SiteKey {
    pub kind: SiteKind,
    pub definition: Option<Span>,
    pub call_site: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct SiteStats {
    pub key: SiteKey,
    pub calls: u64,
    pub inclusive_ns: u64,
    pub exclusive_ns: u64,
    pub blocked_ns: u64,
    pub alloc_count: u64,
    pub alloc_bytes: u64,
    pub children: FxHashMap<u32, u64>,
}

#[derive(Clone, Debug)]
pub struct OpaqueRegion {
    pub kind: &'static str,
    pub span: Option<Span>,
    pub wall_ns: u64,
    pub count: u64,
    pub overlapping: bool,
}

#[derive(Clone, Debug)]
pub struct StackPath { pub parent: Option<u32>, pub site: u32, pub calls: u64, pub exclusive_ns: u64 }

#[derive(Clone, Debug)]
struct Frame {
    site: u32,
    path: u32,
    start: Instant,
    child_ns: u64,
    blocked_ns: u64,
    child_blocked_ns: u64,
    alloc_count: usize,
    alloc_bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ProfileState {
    pub dropped: u64,
    pub sites: Vec<SiteStats>,
    pub opaque: Vec<OpaqueRegion>,
    pub paths: Vec<StackPath>,
    site_index: FxHashMap<SiteKey, u32>,
    path_index: FxHashMap<(Option<u32>, u32), u32>,
    stack: Vec<Frame>,
}

impl ProfileState {
    pub fn finish(&mut self) {
        self.dropped = self.dropped.saturating_add(self.stack.len() as u64);
    }

    pub fn merge_worker(&mut self, mut worker: ProfileState, span: Span, wall_ns: u64) {
        worker.finish();
        self.dropped = self.dropped.saturating_add(worker.dropped);
        let mut ids = Vec::with_capacity(worker.sites.len());
        for site in &worker.sites {
            let id = if let Some(&id) = self.site_index.get(&site.key) { id } else {
                let id = self.sites.len() as u32;
                self.sites.push(SiteStats { key: site.key, calls: 0, inclusive_ns: 0, exclusive_ns: 0, blocked_ns: 0, alloc_count: 0, alloc_bytes: 0, children: FxHashMap::default() });
                self.site_index.insert(site.key, id);
                id
            };
            ids.push(id);
            let target = &mut self.sites[id as usize];
            target.calls = target.calls.saturating_add(site.calls);
            target.inclusive_ns = target.inclusive_ns.saturating_add(site.inclusive_ns);
            target.exclusive_ns = target.exclusive_ns.saturating_add(site.exclusive_ns);
            target.blocked_ns = target.blocked_ns.saturating_add(site.blocked_ns);
            target.alloc_count = target.alloc_count.saturating_add(site.alloc_count);
            target.alloc_bytes = target.alloc_bytes.saturating_add(site.alloc_bytes);
        }
        for (index, site) in worker.sites.iter().enumerate() {
            for (&child, &calls) in &site.children {
                let value = self.sites[ids[index] as usize].children.entry(ids[child as usize]).or_default();
                *value = value.saturating_add(calls);
            }
        }
        let root = self.stack.last().map(|frame| (frame.path, frame.site));
        let mut paths = Vec::with_capacity(worker.paths.len());
        for path in &worker.paths {
            let site = ids[path.site as usize];
            let parent = path.parent.map(|id| paths[id as usize]).or(root.map(|v| v.0));
            let id = if let Some(&id) = self.path_index.get(&(parent, site)) { id } else {
                let id = self.paths.len() as u32;
                self.paths.push(StackPath { parent, site, calls: 0, exclusive_ns: 0 });
                self.path_index.insert((parent, site), id);
                id
            };
            self.paths[id as usize].calls = self.paths[id as usize].calls.saturating_add(path.calls);
            self.paths[id as usize].exclusive_ns = self.paths[id as usize].exclusive_ns.saturating_add(path.exclusive_ns);
            paths.push(id);
        }
        if let Some((_, parent)) = root {
            for path in worker.paths.iter().filter(|path| path.parent.is_none()) {
                let calls = path.calls;
                let entry = self.sites[parent as usize].children.entry(ids[path.site as usize]).or_default();
                *entry = entry.saturating_add(calls);
            }
        }
        for region in worker.opaque {
            if let Some(existing) = self.opaque.iter_mut().find(|r| r.kind == region.kind && r.span == region.span && r.overlapping == region.overlapping) {
                existing.wall_ns = existing.wall_ns.saturating_add(region.wall_ns);
                existing.count = existing.count.saturating_add(region.count);
            } else { self.opaque.push(region); }
        }
        self.opaque("parallel-workers", Some(span), wall_ns, true);
    }

    pub fn enter(&mut self, key: SiteKey) {
        let site = if let Some(&id) = self.site_index.get(&key) { id } else {
            let id = self.sites.len() as u32;
            self.sites.push(SiteStats { key, calls: 0, inclusive_ns: 0, exclusive_ns: 0, blocked_ns: 0, alloc_count: 0, alloc_bytes: 0, children: FxHashMap::default() });
            self.site_index.insert(key, id);
            id
        };
        let parent = self.stack.last().map(|frame| frame.path);
        let path = if let Some(&id) = self.path_index.get(&(parent, site)) { id } else {
            let id = self.paths.len() as u32;
            self.paths.push(StackPath { parent, site, calls: 0, exclusive_ns: 0 });
            self.path_index.insert((parent, site), id);
            id
        };
        if let Some(frame) = self.stack.last() {
            *self.sites[frame.site as usize].children.entry(site).or_default() += 1;
        }
        self.sites[site as usize].calls += 1;
        self.paths[path as usize].calls += 1;
        let alloc = crate::mem_track::snapshot();
        self.stack.push(Frame { site, path, start: Instant::now(), child_ns: 0, blocked_ns: 0, child_blocked_ns: 0, alloc_count: alloc.alloc_count, alloc_bytes: alloc.alloc_bytes });
    }

    pub fn exit(&mut self) {
        let Some(frame) = self.stack.pop() else { return };
        let elapsed = nanos(frame.start);
        let active = elapsed.saturating_sub(frame.child_ns).saturating_sub(frame.blocked_ns);
        let alloc = crate::mem_track::snapshot();
        let site = &mut self.sites[frame.site as usize];
        site.inclusive_ns = site.inclusive_ns.saturating_add(elapsed);
        site.exclusive_ns = site.exclusive_ns.saturating_add(active);
        site.blocked_ns = site.blocked_ns.saturating_add(frame.blocked_ns).saturating_add(frame.child_blocked_ns);
        site.alloc_count = site.alloc_count.saturating_add(alloc.alloc_count.saturating_sub(frame.alloc_count) as u64);
        site.alloc_bytes = site.alloc_bytes.saturating_add(alloc.alloc_bytes.saturating_sub(frame.alloc_bytes) as u64);
        self.paths[frame.path as usize].exclusive_ns = self.paths[frame.path as usize].exclusive_ns.saturating_add(active);
        if let Some(parent) = self.stack.last_mut() {
            parent.child_ns = parent.child_ns.saturating_add(elapsed);
            parent.child_blocked_ns = parent.child_blocked_ns.saturating_add(frame.blocked_ns).saturating_add(frame.child_blocked_ns);
        }
    }

    pub fn opaque(&mut self, kind: &'static str, span: Option<Span>, wall_ns: u64, overlapping: bool) {
        if !overlapping && let Some(frame) = self.stack.last_mut() { frame.blocked_ns = frame.blocked_ns.saturating_add(wall_ns); }
        if let Some(region) = self.opaque.iter_mut().find(|region| region.kind == kind && region.span == span && region.overlapping == overlapping) {
            region.wall_ns = region.wall_ns.saturating_add(wall_ns);
            region.count += 1;
        } else { self.opaque.push(OpaqueRegion { kind, span, wall_ns, count: 1, overlapping }); }
    }
}

pub fn nanos(start: Instant) -> u64 { start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX) }
