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
pub struct StackPath { pub parent: Option<u32>, pub site: u32, pub exclusive_ns: u64 }

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
            self.paths.push(StackPath { parent, site, exclusive_ns: 0 });
            self.path_index.insert((parent, site), id);
            id
        };
        if let Some(frame) = self.stack.last() {
            *self.sites[frame.site as usize].children.entry(site).or_default() += 1;
        }
        self.sites[site as usize].calls += 1;
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
