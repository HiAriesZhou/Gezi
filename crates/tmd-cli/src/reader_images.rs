//! Session images: bounded background IO/preparation and owned Kitty placements.
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, Write};
use std::sync::{Arc, mpsc};

use tmd_core::{Document, ImageCell, ImageLayout, Line};

use crate::cli::{Cli, ImageMode, RemoteImages};
use crate::image_loader::{self, Raster, Source};
use crate::terminal_io::direct_kaku;

const CACHE_BYTES: usize = 64 * 1024 * 1024;
const UPLOAD_BYTES: usize = 128 * 1024;

enum State {
    Idle,
    Loading,
    Ready(Arc<Raster>),
    Failed,
}

struct Entry {
    source: Arc<Source>,
    state: State,
    touched: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    resource: usize,
    width: u32,
    height: u32,
}

struct Variant {
    serial: u32,
    data: Option<String>,
    failed: bool,
    touched: u64,
}

enum Reply {
    Loaded(usize, Result<Raster, String>),
    Prepared(Key, Result<String, String>),
}

pub struct Viewport {
    pub start: usize,
    pub end: usize,
    pub origin: u16,
    pub height: usize,
    pub show: bool,
}

pub struct Images {
    enabled: bool,
    mapping: Vec<Option<usize>>,
    entries: Vec<Entry>,
    sizes: Vec<Option<(u32, u32)>>,
    failed: Vec<bool>,
    variants: HashMap<Key, Variant>,
    sender: mpsc::SyncSender<Reply>,
    receiver: mpsc::Receiver<Reply>,
    active: usize,
    tick: u64,
    uploaded: HashSet<Key>,
    placements: HashSet<Key>,
    retired: Vec<u32>,
    uploading: Option<(Key, usize)>,
    next_id: u32,
    visible: HashSet<Key>,
}

impl Images {
    pub fn new(cli: &Cli, doc: &Document) -> Self {
        Self::configured(
            cli,
            doc,
            cli.images != ImageMode::Off && direct_kaku() && !doc.images().is_empty(),
        )
    }

    fn configured(cli: &Cli, doc: &Document, enabled: bool) -> Self {
        let mut entries = Vec::new();
        let mut lookup = HashMap::new();
        let mapping = doc
            .images()
            .iter()
            .map(|image| {
                if !enabled {
                    return None;
                }
                let source = image_loader::source(
                    &image.destination,
                    cli.file.as_deref()?,
                    cli.remote_images == RemoteImages::On,
                )
                .ok()?;
                if let Some(&index) = lookup.get(&source) {
                    return Some(index);
                }
                if entries.len() == 1024 {
                    return None;
                }
                let index = entries.len();
                lookup.insert(source.clone(), index);
                entries.push(Entry {
                    source: Arc::new(source),
                    state: State::Idle,
                    touched: 0,
                });
                Some(index)
            })
            .collect();
        let (sender, receiver) = mpsc::sync_channel(2);
        Self {
            enabled,
            mapping,
            entries,
            sizes: vec![None; doc.images().len()],
            failed: vec![false; doc.images().len()],
            variants: HashMap::new(),
            sender,
            receiver,
            active: 0,
            tick: 0,
            uploaded: HashSet::new(),
            placements: HashSet::new(),
            retired: Vec::new(),
            uploading: None,
            visible: HashSet::new(),
            next_id: (RandomState::new().hash_one(std::process::id()) as u32) % (u32::MAX / 2) + 1,
        }
    }

    pub fn layout(&self) -> Option<ImageLayout<'_>> {
        self.enabled.then(|| {
            let (cell_width, cell_height) = cell_size();
            ImageLayout {
                cell_width,
                cell_height,
                sizes: &self.sizes,
                failed: &self.failed,
            }
        })
    }

    fn key(&self, cell: ImageCell) -> Option<Key> {
        Some(Key {
            resource: self.mapping.get(cell.index).copied().flatten()?,
            width: cell.pixel_width,
            height: cell.pixel_height,
        })
    }

    fn bytes(&self) -> usize {
        let rasters: usize = self
            .entries
            .iter()
            .map(|entry| match &entry.state {
                State::Ready(raster) => raster.pixels.len(),
                _ => 0,
            })
            .sum();
        rasters
            + self
                .variants
                .iter()
                .filter_map(|(key, variant)| {
                    variant
                        .data
                        .as_ref()
                        .map(|data| data.len().max(key.width as usize * key.height as usize * 4))
                })
                .sum::<usize>()
    }

    fn make_room(&mut self, cost: usize) -> bool {
        while self.bytes() + cost > CACHE_BYTES || self.variants.len() >= 256 {
            let victim = self
                .variants
                .iter()
                .filter(|(key, variant)| {
                    (variant.data.is_some() || variant.failed)
                        && !self.visible.contains(key)
                        && self.uploading.is_none_or(|(current, _)| current != **key)
                })
                .min_by_key(|(_, variant)| variant.touched)
                .map(|(key, _)| *key);
            if let Some(key) = victim {
                let variant = self.variants.remove(&key).unwrap();
                if self.uploaded.remove(&key) {
                    self.retired.push(variant.serial);
                }
                continue;
            }
            let victim = self
                .entries
                .iter()
                .enumerate()
                .filter(|(index, entry)| {
                    matches!(entry.state, State::Ready(_))
                        && !self.visible.iter().any(|key| key.resource == *index)
                        && !self.variants.iter().any(|(key, variant)| {
                            key.resource == *index && variant.data.is_none() && !variant.failed
                        })
                })
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(index, _)| index);
            let Some(index) = victim else { return false };
            self.entries[index].state = State::Idle;
        }
        true
    }

    fn fail(&mut self, index: usize) {
        self.entries[index].state = State::Failed;
        for (reference, resource) in self.mapping.iter().enumerate() {
            if *resource == Some(index) {
                self.sizes[reference] = None;
                self.failed[reference] = true;
            }
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(reply) = self.receiver.try_recv() {
            self.active -= 1;
            match reply {
                Reply::Loaded(index, Ok(raster)) if self.make_room(raster.pixels.len()) => {
                    for (reference, resource) in self.mapping.iter().enumerate() {
                        if *resource == Some(index) {
                            self.sizes[reference] = Some((raster.width, raster.height));
                        }
                    }
                    self.entries[index].state = State::Ready(Arc::new(raster));
                }
                Reply::Loaded(index, _) => self.fail(index),
                Reply::Prepared(key, Ok(data))
                    if self.make_room(
                        data.len().max(key.width as usize * key.height as usize * 4),
                    ) =>
                {
                    if let Some(variant) = self.variants.get_mut(&key) {
                        variant.data = Some(data);
                    }
                }
                Reply::Prepared(key, _) => {
                    if let Some(variant) = self.variants.get_mut(&key) {
                        variant.failed = true;
                    }
                    self.fail(key.resource);
                }
            }
            changed = true;
        }
        changed
    }

    fn resources(&self, lines: &[Line], from: usize, to: usize) -> Vec<usize> {
        let mut result = Vec::new();
        for span in lines
            .iter()
            .take(to)
            .skip(from)
            .flat_map(|line| &line.spans)
        {
            let reference = span
                .image_cell
                .map(|cell| cell.index)
                .or_else(|| span.image.as_ref().map(|source| source.index));
            if let Some(index) =
                reference.and_then(|reference| self.mapping.get(reference).copied().flatten())
                && !result.contains(&index)
            {
                result.push(index);
            }
        }
        result
    }

    fn keys(&self, lines: &[Line], start: usize, end: usize) -> Vec<Key> {
        let mut keys = Vec::new();
        for cell in lines
            .iter()
            .take(end)
            .skip(start)
            .flat_map(|line| &line.spans)
            .filter_map(|span| span.image_cell)
        {
            if let Some(key) = self.key(cell)
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        keys
    }

    pub fn request(&mut self, lines: &[Line], start: usize, end: usize, height: usize) {
        if !self.enabled {
            return;
        }
        self.tick += 1;
        let keys = self.keys(lines, start, end);
        self.visible = keys.iter().copied().collect();
        for key in &keys {
            self.entries[key.resource].touched = self.tick;
            if let Some(variant) = self.variants.get_mut(key) {
                variant.touched = self.tick;
                continue;
            }
            if self.active == 2 {
                continue;
            }
            if let State::Ready(raster) = &self.entries[key.resource].state {
                let raster = raster.clone();
                let sender = self.sender.clone();
                let key = *key;
                if !self.make_room(0) {
                    continue;
                }
                let Some(next_id) = self.next_id.checked_add(1) else {
                    continue;
                };
                self.variants.insert(
                    key,
                    Variant {
                        serial: self.next_id,
                        data: None,
                        failed: false,
                        touched: self.tick,
                    },
                );
                self.next_id = next_id;
                self.active += 1;
                std::thread::spawn(move || {
                    let result = std::panic::catch_unwind(|| {
                        image_loader::prepare(&raster, key.width, key.height)
                    })
                    .unwrap_or_else(|_| Err("image preparation failed".into()));
                    let _ = sender.send(Reply::Prepared(key, result));
                });
            }
        }
        let mut resources = self.resources(lines, start, end);
        for &index in &resources {
            self.entries[index].touched = self.tick;
        }
        for index in self.resources(
            lines,
            start.saturating_sub(height),
            end.saturating_add(height),
        ) {
            if !resources.contains(&index) {
                resources.push(index);
            }
        }
        for index in resources {
            if self.active == 2 {
                break;
            }
            if !matches!(self.entries[index].state, State::Idle) {
                continue;
            }
            self.active += 1;
            self.entries[index].state = State::Loading;
            let sender = self.sender.clone();
            let source = self.entries[index].source.clone();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| image_loader::load(&source))
                    .unwrap_or_else(|_| Err("image decoder failed".into()));
                let _ = sender.send(Reply::Loaded(index, result));
            });
        }
    }

    pub fn needs_upload(&self, lines: &[Line], start: usize, end: usize) -> bool {
        self.uploading.is_some()
            || self.keys(lines, start, end).iter().any(|key| {
                !self.uploaded.contains(key)
                    && self
                        .variants
                        .get(key)
                        .is_some_and(|variant| variant.data.is_some())
            })
    }

    /// Draw exact pixels, without c/r stretching. Cropping uses cell pixels,
    /// so transparent cell padding never changes a badge's aspect ratio.
    pub fn paint(&mut self, out: &mut String, lines: &[Line], viewport: Viewport) {
        let Viewport {
            start,
            end,
            origin,
            height,
            show,
        } = viewport;
        if !self.enabled {
            return;
        }
        out.push_str("\x1b7");
        for key in self.placements.drain() {
            if let Some(variant) = self.variants.get(&key) {
                out.push_str(&format!("\x1b_Ga=d,d=i,i={},q=2;\x1b\\", variant.serial));
            }
        }
        for id in self.retired.drain(..) {
            out.push_str(&format!("\x1b_Ga=d,d=I,i={id},q=2;\x1b\\"));
        }
        if show && self.uploading.is_none() {
            self.uploading = self
                .keys(lines, start, end)
                .into_iter()
                .find(|key| {
                    !self.uploaded.contains(key)
                        && self
                            .variants
                            .get(key)
                            .is_some_and(|variant| variant.data.is_some())
                })
                .map(|key| (key, 0));
        }
        if let Some((key, offset)) = self.uploading
            && let Some(variant) = self.variants.get(&key)
            && let Some(data) = &variant.data
        {
            let stop = (offset + UPLOAD_BYTES).min(data.len());
            let mut at = offset;
            while at < stop {
                let to = (at + 4096).min(stop);
                let more = usize::from(to < data.len());
                if at == 0 {
                    out.push_str(&format!(
                        "\x1b_Ga=t,t=d,f=100,i={},q=2,m={more};",
                        variant.serial
                    ));
                } else {
                    out.push_str(&format!("\x1b_Gq=2,m={more};"));
                }
                out.push_str(&data[at..to]);
                out.push_str("\x1b\\");
                at = to;
            }
            if stop == data.len() {
                self.uploading = None;
                self.uploaded.insert(key);
            } else {
                self.uploading = Some((key, stop));
            }
        }
        if show {
            let mut seen = HashSet::new();
            let mut row = 0;
            let (_, cell_height) = cell_size();
            for line in lines.iter().take(end).skip(start) {
                let mut column = 0;
                for span in &line.spans {
                    if let Some(cell) = span.image_cell
                        && seen.insert(cell.index)
                        && let Some(key) = self.key(cell)
                        && self.uploaded.contains(&key)
                        && let Some(variant) = self.variants.get(&key)
                    {
                        let y = u32::from(cell.row) * u32::from(cell_height);
                        let pixels = key
                            .height
                            .saturating_sub(y)
                            .min(height.saturating_sub(row) as u32 * u32::from(cell_height));
                        if pixels > 0 {
                            out.push_str(&format!(
                                "\x1b[{};{}H\x1b_Ga=p,i={},p={},x=0,y={y},w={},h={pixels},C=1,q=2;\x1b\\",
                                row + 2, usize::from(origin) + column, variant.serial, cell.index + 1, key.width,
                            ));
                            self.placements.insert(key);
                        }
                    }
                    column += span.width();
                }
                row += line.physical_height();
            }
        }
        out.push_str("\x1b8");
    }
}

impl Drop for Images {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        let mut out = io::stdout().lock();
        if !self.uploaded.is_empty() || self.uploading.is_some() {
            let _ = write!(out, "\x18\x1b\\");
        }
        if self.uploading.is_some() {
            // Finish/discard partial chunks before another application uploads.
            let _ = write!(out, "\x1b_Gq=2,m=0;\x1b\\");
        }
        for key in self
            .uploaded
            .iter()
            .copied()
            .chain(self.uploading.map(|(key, _)| key))
        {
            if let Some(variant) = self.variants.get(&key) {
                let _ = write!(out, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", variant.serial);
            }
        }
        for id in &self.retired {
            let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2;\x1b\\");
        }
        let _ = out.flush();
    }
}

fn cell_size() -> (u16, u16) {
    #[cfg(unix)]
    {
        let mut size: nix::libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { nix::libc::ioctl(1, nix::libc::TIOCGWINSZ, &mut size) } == 0
            && size.ws_col > 0
            && size.ws_row > 0
            && size.ws_xpixel > 0
            && size.ws_ypixel > 0
        {
            return (
                (size.ws_xpixel / size.ws_col).max(1),
                (size.ws_ypixel / size.ws_row).max(1),
            );
        }
    }
    (8, 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tmd_core::{RenderOptions, Theme};

    fn fixture(markdown: &str, options: &[&str]) -> (Document, Images) {
        let mut args = vec!["tmd", "/tmp/docs/article.md"];
        args.extend_from_slice(options);
        let cli = Cli::try_parse_from(args).unwrap();
        let doc = Document::new(markdown, Theme::default());
        let images = Images::configured(&cli, &doc, cli.images != ImageMode::Off);
        (doc, images)
    }

    #[test]
    fn remote_off_still_allows_local_and_duplicate_sources_share_cache() {
        let (_, mut images) = fixture(
            "![a](a.png) ![b](a.png) ![r](https://example.test/badge)",
            &["--remote-images", "off"],
        );
        assert_eq!(images.mapping, [Some(0), Some(0), None]);
        assert_eq!(images.entries.len(), 1);
        images.enabled = false;
        let (_, images) = fixture("![a](a.png)", &["--images", "off"]);
        assert_eq!(images.mapping, [None]);
        assert!(images.layout().is_none());
    }

    #[test]
    fn cropped_viewport_uses_exact_pixels_without_cell_stretching() {
        let (doc, mut images) = fixture("![a](a.png)\n\n# after", &[]);
        let raster = image_loader::decode(br#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="160"><rect width="80" height="160" fill="red"/></svg>"#).unwrap();
        images.sizes[0] = Some((80, 160));
        let key = Key {
            resource: 0,
            width: 80,
            height: 160,
        };
        let data = image_loader::prepare(&raster, key.width, key.height).unwrap();
        images.variants.insert(
            key,
            Variant {
                serial: 12345,
                data: Some(data),
                failed: false,
                touched: 0,
            },
        );
        let lines = doc.layout_with_images(
            20,
            RenderOptions::default(),
            &ImageLayout {
                cell_width: 8,
                cell_height: 16,
                sizes: &images.sizes,
                failed: &[],
            },
        );
        let mut frame = String::new();
        images.paint(
            &mut frame,
            &lines,
            Viewport {
                start: 3,
                end: 7,
                origin: 27,
                height: 4,
                show: true,
            },
        );
        assert!(frame.contains("y=48,w=80,h=64,C=1"));
        assert!(frame.contains("\x1b[2;27H"));
        assert!(frame.contains("a=t,t=d,f=100"));
        assert!(!frame.contains(",c=") && !frame.contains(",r="));
        frame.clear();
        images.paint(
            &mut frame,
            &lines,
            Viewport {
                start: 3,
                end: 7,
                origin: 27,
                height: 4,
                show: false,
            },
        );
        assert!(frame.contains("a=d,d=i,i=12345"));
        assert!(!frame.contains("a=p") && !frame.contains("a=t"));
        assert!(!frame.contains("d=A") && !frame.contains("d=a"));
        images.enabled = false;
    }

    #[test]
    fn cache_eviction_releases_only_unprotected_resources() {
        let (_, mut images) = fixture("![a](a.png) ![b](b.png)", &[]);
        let small = image_loader::decode(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#,
        )
        .unwrap();
        images.entries[0].state = State::Ready(Arc::new(small));
        let key = Key {
            resource: 0,
            width: 10,
            height: 10,
        };
        images.visible.insert(key);
        assert!(!images.make_room(CACHE_BYTES));
        assert!(matches!(images.entries[0].state, State::Ready(_)));
        images.visible.clear();
        assert!(images.make_room(CACHE_BYTES));
        assert!(matches!(images.entries[0].state, State::Idle));
        images.enabled = false;
    }
}
