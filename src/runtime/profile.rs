use crate::source::Span;
use rustc_hash::FxHashMap;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SiteKind { Proc, Pure, Module, ModulePrepare, ModuleInit, PrepareParse, PrepareCheck, PrepareLower, Method, Core }

impl SiteKind {
    pub fn phase_name(self) -> Option<&'static str> {
        match self { Self::PrepareParse => Some("parse"), Self::PrepareCheck => Some("check"), Self::PrepareLower => Some("lower"), _ => None }
    }

    pub fn name(self) -> &'static str {
        match self { Self::Proc => "proc", Self::Pure => "pure", Self::Module => "module", Self::ModulePrepare => "module-prepare", Self::ModuleInit => "module-init", Self::PrepareParse | Self::PrepareCheck | Self::PrepareLower => "prepare-phase", Self::Method => "method", Self::Core => "core" }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SiteKey {
    pub kind: SiteKind,
    pub definition: Option<Span>,
    pub call_site: Option<Span>,
    pub module: Option<std::sync::Arc<str>>,
    pub function_name: Option<crate::symbol::Name>,
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
pub struct StackPath { pub parent: Option<u32>, pub site: u32, pub calls: u64, pub exclusive_ns: u64, pub inclusive_ns: u64, pub worker: bool }

#[derive(Clone, Debug, Default)]
pub struct WorkerTiming { pub items: u64, pub start_delay_ns: u64, pub setup_ns: u64, pub busy_ns: u64, pub wall_ns: u64, pub worker_alloc_peak_bytes: u64 }

#[derive(Clone, Debug)]
pub struct StageTiming { pub span: Span, pub path: Option<u32>, pub executions: u64, pub items: u64, pub workers: u64, pub worker: Vec<WorkerTiming>, pub critical_ns: u64, pub parent_setup_ns: u64, pub parent_spawn_ns: u64, pub parent_wait_ns: u64, pub buffered_results: u64, pub buffered_bytes: u64, pub buffered_item_max_bytes: u64, pub input_bytes: u64 }

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
    pub stages: Vec<StageTiming>,
    site_index: FxHashMap<SiteKey, u32>,
    path_index: FxHashMap<(Option<u32>, u32, bool), u32>,
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
                self.sites.push(SiteStats { key: site.key.clone(), calls: 0, inclusive_ns: 0, exclusive_ns: 0, blocked_ns: 0, alloc_count: 0, alloc_bytes: 0, children: FxHashMap::default() });
                self.site_index.insert(site.key.clone(), id);
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
            let id = if let Some(&id) = self.path_index.get(&(parent, site, true)) { id } else {
                let id = self.paths.len() as u32;
                self.paths.push(StackPath { parent, site, calls: 0, exclusive_ns: 0, inclusive_ns: 0, worker: true });
                self.path_index.insert((parent, site, true), id);
                id
            };
            self.paths[id as usize].calls = self.paths[id as usize].calls.saturating_add(path.calls);
            self.paths[id as usize].exclusive_ns = self.paths[id as usize].exclusive_ns.saturating_add(path.exclusive_ns);
            self.paths[id as usize].inclusive_ns = self.paths[id as usize].inclusive_ns.saturating_add(path.inclusive_ns);
            paths.push(id);
        }
        for mut stage in worker.stages {
            stage.path = stage.path.map(|id| paths[id as usize]);
            self.merge_stage(stage);
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

    pub fn stage(&mut self, mut stage: StageTiming) {
        stage.path = self.stack.last().map(|frame| frame.path);
        self.merge_stage(stage);
    }

    fn merge_stage(&mut self, stage: StageTiming) {
        if let Some(old) = self.stages.iter_mut().find(|old| old.span == stage.span) {
            old.executions += stage.executions;
            old.items += stage.items;
            old.workers += stage.workers;
            old.critical_ns = old.critical_ns.max(stage.critical_ns);
            old.parent_setup_ns += stage.parent_setup_ns;
            old.parent_spawn_ns += stage.parent_spawn_ns;
            old.parent_wait_ns += stage.parent_wait_ns;
            old.buffered_results += stage.buffered_results;
            old.buffered_bytes += stage.buffered_bytes;
            old.input_bytes += stage.input_bytes;
            old.buffered_item_max_bytes = old.buffered_item_max_bytes.max(stage.buffered_item_max_bytes);
            old.worker.resize_with(old.worker.len().max(stage.worker.len()), WorkerTiming::default);
            for (a, b) in old.worker.iter_mut().zip(stage.worker) {
                a.items += b.items; a.start_delay_ns += b.start_delay_ns; a.setup_ns += b.setup_ns; a.busy_ns += b.busy_ns; a.wall_ns += b.wall_ns; a.worker_alloc_peak_bytes = a.worker_alloc_peak_bytes.max(b.worker_alloc_peak_bytes);
            }
        } else { self.stages.push(stage); }
    }

    pub fn enter(&mut self, key: SiteKey) {
        let site = if let Some(&id) = self.site_index.get(&key) { id } else {
            let id = self.sites.len() as u32;
            self.sites.push(SiteStats { key: key.clone(), calls: 0, inclusive_ns: 0, exclusive_ns: 0, blocked_ns: 0, alloc_count: 0, alloc_bytes: 0, children: FxHashMap::default() });
            self.site_index.insert(key, id);
            id
        };
        let parent = self.stack.last().map(|frame| frame.path);
        let path = if let Some(&id) = self.path_index.get(&(parent, site, false)) { id } else {
            let id = self.paths.len() as u32;
            self.paths.push(StackPath { parent, site, calls: 0, exclusive_ns: 0, inclusive_ns: 0, worker: false });
            self.path_index.insert((parent, site, false), id);
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
        self.paths[frame.path as usize].inclusive_ns = self.paths[frame.path as usize].inclusive_ns.saturating_add(elapsed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn key(kind: SiteKind) -> SiteKey {
        SiteKey { kind, definition: None, call_site: None, module: None, function_name: None }
    }

    fn timed(state: &mut ProfileState, kind: SiteKind, sleep: std::time::Duration) {
        state.enter(key(kind));
        std::thread::sleep(sleep);
        state.exit();
    }

    #[test]
    fn worker_paths_stay_separate_from_serial_paths_at_the_same_site() {
        let short = std::time::Duration::from_millis(1);
        let long = std::time::Duration::from_millis(40);
        let mut state = ProfileState::default();
        state.enter(key(SiteKind::Proc));
        timed(&mut state, SiteKind::Pure, short);
        let mut worker = ProfileState::default();
        timed(&mut worker, SiteKind::Pure, long);
        state.merge_worker(worker, crate::source::Span::at(SourceId::new(0), 0), long.as_nanos() as u64);
        state.exit();

        let parent = state.paths.iter().position(|path| path.parent.is_none()).expect("parent path") as u32;
        let children: Vec<_> = state.paths.iter().filter(|path| path.parent == Some(parent)).collect();
        assert_eq!(children.len(), 2, "{children:?}");
        let serial = children.iter().find(|path| !path.worker).expect("serial child path");
        let merged = children.iter().find(|path| path.worker).expect("worker child path");
        assert_eq!((serial.calls, merged.calls), (1, 1));
        assert!(serial.inclusive_ns < long.as_nanos() as u64, "serial path absorbed worker time: {serial:?}");
        assert!(merged.inclusive_ns >= long.as_nanos() as u64, "{merged:?}");
        let root = &state.paths[parent as usize];
        assert_eq!(root.inclusive_ns, root.exclusive_ns + serial.inclusive_ns);
    }
}
