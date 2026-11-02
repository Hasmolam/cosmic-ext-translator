use std::os::fd::OwnedFd;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use image::RgbaImage;
use pipewire as pw;
use pw::spa;
use spa::pod::Pod;
use tracing::{debug, info, warn};

use crate::error::{CaptureError, Result};
use super::roi::{CaptureRegion, Roi, RoiPreset};

#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub image: RgbaImage,
    pub timestamp: Instant,
    pub original_width: u32,
    pub original_height: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum PipewireCommand {
    UpdateRegion(CaptureRegion),
    UpdateRoi(Option<Roi>),
    Stop,
}

struct StreamUserData {
    format: spa::param::video::VideoInfoRaw,
    region: Arc<RwLock<CaptureRegion>>,
    frame_sender: tokio::sync::watch::Sender<Option<CapturedFrame>>,
    snapshot_sender: tokio::sync::watch::Sender<Option<RgbaImage>>,
    snapshot_requested: Arc<std::sync::atomic::AtomicBool>,
    last_capture_time: Instant,
}

#[derive(Clone)]
pub struct SnapshotRequester {
    pub snapshot_requested: Arc<std::sync::atomic::AtomicBool>,
    pub snapshot_receiver: tokio::sync::watch::Receiver<Option<RgbaImage>>,
}

impl SnapshotRequester {
    pub async fn get_full_snapshot(&self) -> Option<RgbaImage> {
        let mut rx = self.snapshot_receiver.clone();
        self.snapshot_requested.store(true, std::sync::atomic::Ordering::SeqCst);
        tokio::time::timeout(std::time::Duration::from_millis(1500), async {
            loop {
                if rx.changed().await.is_err() {
                    return None;
                }
                if let Some(ref img) = *rx.borrow_and_update() {
                    return Some(img.clone());
                }
            }
        })
        .await
        .unwrap_or_default()
    }
}

pub struct PipewireCaptureHandle {
    command_sender: pw::channel::Sender<PipewireCommand>,
    thread_handle: Option<std::thread::JoinHandle<Result<()>>>,
    pub frame_receiver: tokio::sync::watch::Receiver<Option<CapturedFrame>>,
    pub snapshot_receiver: tokio::sync::watch::Receiver<Option<RgbaImage>>,
    region: Arc<RwLock<CaptureRegion>>,
    snapshot_requested: Arc<std::sync::atomic::AtomicBool>,
}

impl PipewireCaptureHandle {
    pub fn update_region(&self, region: CaptureRegion) {
        if let Ok(mut current) = self.region.write() {
            *current = region;
        }
        let _ = self.command_sender.send(PipewireCommand::UpdateRegion(region));
    }

    pub fn update_roi(&self, roi: Option<Roi>) {
        let current_ignore = self.region.read().ok().and_then(|r| r.ignore_roi);
        let region = CaptureRegion {
            preset: if roi.is_some() {
                RoiPreset::Custom
            } else {
                RoiPreset::FullScreen
            },
            custom: roi,
            ignore_roi: current_ignore,
        };
        self.update_region(region);
    }

    pub fn update_ignore_roi(&self, ignore_roi: Option<Roi>) {
        if let Ok(mut current) = self.region.write() {
            current.ignore_roi = ignore_roi;
            let region = *current;
            let _ = self.command_sender.send(PipewireCommand::UpdateRegion(region));
        }
    }

    pub fn snapshot_requester(&self) -> SnapshotRequester {
        SnapshotRequester {
            snapshot_requested: Arc::clone(&self.snapshot_requested),
            snapshot_receiver: self.snapshot_receiver.clone(),
        }
    }

    /// Requests an uncropped full-frame snapshot of the entire game window or screen.
    pub async fn get_full_snapshot(&self) -> Option<RgbaImage> {
        self.snapshot_requester().get_full_snapshot().await
    }

    pub fn stop(&mut self) -> Result<()> {
        let _ = self.command_sender.send(PipewireCommand::Stop);
        if let Some(handle) = self.thread_handle.take() {
            handle
                .join()
                .map_err(|e| CaptureError::ThreadDied(format!("{e:?}")))?
        } else {
            Ok(())
        }
    }
}

impl Drop for PipewireCaptureHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn spawn_pipewire_capture_thread(
    fd: OwnedFd,
    node_id: u32,
    initial_region: CaptureRegion,
) -> Result<(
    PipewireCaptureHandle,
    tokio::sync::watch::Receiver<Option<CapturedFrame>>,
)> {
    let (frame_sender, frame_receiver) = tokio::sync::watch::channel(None);
    let (snapshot_sender, snapshot_receiver) = tokio::sync::watch::channel(None);
    let snapshot_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let snapshot_requested_clone = Arc::clone(&snapshot_requested);
    let (pw_sender, pw_receiver) = pw::channel::channel::<PipewireCommand>();
    let region = Arc::new(RwLock::new(initial_region));
    let region_clone = Arc::clone(&region);

    let thread_handle = std::thread::Builder::new()
        .name("pw-capture-loop".into())
        .spawn(move || {
            run_pipewire_loop(
                fd,
                node_id,
                region_clone,
                pw_receiver,
                frame_sender,
                snapshot_sender,
                snapshot_requested_clone,
            )
        })
        .map_err(|e| CaptureError::ThreadDied(format!("Failed to spawn PipeWire thread: {e}")))?;

    let handle = PipewireCaptureHandle {
        command_sender: pw_sender,
        thread_handle: Some(thread_handle),
        frame_receiver: frame_receiver.clone(),
        snapshot_receiver,
        region,
        snapshot_requested,
    };

    Ok((handle, frame_receiver))
}

fn run_pipewire_loop(
    fd: OwnedFd,
    node_id: u32,
    region: Arc<RwLock<CaptureRegion>>,
    pw_receiver: pw::channel::Receiver<PipewireCommand>,
    frame_sender: tokio::sync::watch::Sender<Option<CapturedFrame>>,
    snapshot_sender: tokio::sync::watch::Sender<Option<RgbaImage>>,
    snapshot_requested: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| CaptureError::PipeWire(format!("MainLoop creation error: {e}")))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| CaptureError::PipeWire(format!("Context creation error: {e}")))?;
    let core = context
        .connect_fd_rc(fd, None)
        .map_err(|e| CaptureError::PipeWire(format!("Connect FD error: {e}")))?;

    // Attach command receiver to mainloop
    let mainloop_weak = mainloop.clone();
    let _receiver = pw_receiver.attach(mainloop.loop_(), move |command| match command {
        PipewireCommand::UpdateRegion(_) => {
            debug!("Region updated in PipeWire loop");
        }
        PipewireCommand::UpdateRoi(_) => {
            debug!("ROI updated in PipeWire loop");
        }
        PipewireCommand::Stop => {
            info!("Stop command received, exiting PipeWire loop");
            mainloop_weak.quit();
        }
    });

    let user_data = StreamUserData {
        format: Default::default(),
        region,
        frame_sender,
        snapshot_sender,
        snapshot_requested,
        last_capture_time: Instant::now()
            .checked_sub(std::time::Duration::from_millis(100))
            .unwrap_or_else(Instant::now),
    };

    let stream = pw::stream::StreamBox::new(
        &core,
        "cosmic-ext-lingua-capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|e| CaptureError::PipeWire(format!("StreamBox error: {e}")))?;

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .state_changed(|_, _, old, new| {
            info!("PipeWire stream state: {old:?} -> {new:?}");
        })
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) =
                match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(_) => return,
                };

            if media_type != pw::spa::param::format::MediaType::Video
                || media_subtype != pw::spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            if user_data.format.parse(param).is_ok() {
                info!(
                    "Negotiated video format: {:?}, size: {}x{}",
                    user_data.format.format(),
                    user_data.format.size().width,
                    user_data.format.size().height
                );
            }
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let frame_w = user_data.format.size().width;
            let frame_h = user_data.format.size().height;
            if frame_w == 0 || frame_h == 0 {
                return;
            }

            let is_bgr = matches!(
                user_data.format.format(),
                pw::spa::param::video::VideoFormat::BGRx
                    | pw::spa::param::video::VideoFormat::BGRA
                    | pw::spa::param::video::VideoFormat::BGR
            );

            let data = &mut datas[0];
            let stride = data.chunk().stride();
            let effective_stride = if stride > 0 {
                stride as usize
            } else {
                frame_w as usize * 4
            };

            let Some(slice) = data.data() else {
                return;
            };

            // 1. If a full snapshot was requested (for full game region picker), capture it immediately!
            if user_data.snapshot_requested.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let full_roi = Roi::new(0, 0, frame_w, frame_h);
                if let Ok(full_img) = full_roi.crop_to_rgba(slice, frame_w, frame_h, effective_stride, is_bgr) {
                    let _ = user_data.snapshot_sender.send(Some(full_img));
                }
            }

            // 2. Throttle frame processing to max 20 FPS (50ms interval).
            let now = Instant::now();
            if now.duration_since(user_data.last_capture_time) < std::time::Duration::from_millis(50) {
                return;
            }
            user_data.last_capture_time = now;

            let region = user_data.region.read().ok().map(|r| *r).unwrap_or_default();
            let target_roi = region.compute(frame_w, frame_h).unwrap_or_else(|| Roi::new(0, 0, frame_w, frame_h));

            match target_roi.crop_to_rgba(slice, frame_w, frame_h, effective_stride, is_bgr) {
                Ok(mut image) => {
                    if let Some(ref ignore) = region.ignore_roi {
                        target_roi.mask_region(&mut image, ignore);
                    }
                    let frame = CapturedFrame {
                        image,
                        timestamp: Instant::now(),
                        original_width: frame_w,
                        original_height: frame_h,
                    };
                    // Non-blocking watch send: Drops previous unread frame automatically
                    let _ = user_data.frame_sender.send(Some(frame));
                }
                Err(err) => {
                    warn!("Failed to crop ROI from PipeWire frame: {err}");
                }
            }

            // Buffer is dropped here, which releases it back to PipeWire immediately!
        })
        .register()
        .map_err(|e| CaptureError::PipeWire(format!("Register listener error: {e}")))?;

    // Video format parameters
    let obj = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            pw::spa::param::format::MediaType::Video
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            pw::spa::param::format::MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRA,
            pw::spa::param::video::VideoFormat::RGBx,
            pw::spa::param::video::VideoFormat::RGBA,
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            pw::spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            pw::spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            pw::spa::utils::Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            pw::spa::utils::Fraction { num: 60, denom: 1 },
            pw::spa::utils::Fraction { num: 0, denom: 1 },
            pw::spa::utils::Fraction {
                num: 1000,
                denom: 1
            }
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| CaptureError::PipeWire(format!("Serialize format error: {e:?}")))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values)
        .ok_or_else(|| CaptureError::PipeWire("Pod from_bytes failed".into()))?];

    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| CaptureError::PipeWire(format!("Connect stream error: {e}")))?;

    info!("PipeWire stream connected to node {node_id}, entering mainloop");
    mainloop.run();
    info!("PipeWire mainloop exited cleanly");

    Ok(())
}
