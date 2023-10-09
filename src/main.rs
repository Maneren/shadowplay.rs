//! Record a movie of your screen
//!
//! This simple utility records a movie (video only, no audio) of your screen.
//!
//! It it based entirely on code from [srs](), and has been maintained as an
//! example application to test the [vpx-encode]() library.
//!
//! Note: compile and test in release mode. Otherwise, converting the video
//! frames is too slow for realtime encoding.
//!
//! # Installation
//!
//! This can be installed with:
//!
//! ```sh
//! cargo install record-screen
//! ```
//!
//! Don't forget to install `libvpx`.
//!
//! # Video Format
//!
//! The video is stored as a WebM file.
//!
//! # Contributing
//!
//! All contributions are appreciated.
#![feature(array_chunks)]
#![feature(is_some_and)]
#![warn(clippy::pedantic)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::doc_markdown)]

mod convert;

use std::{
  env, fmt,
  fs::{File, OpenOptions},
  io,
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thread,
  time::{Duration, Instant},
};

use clap::{Parser, ValueEnum};
use quest::Boxes;
use scrap::{Capturer, Display};
use webm::{mux, mux::Track};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
  /// Output folder
  ///
  /// Defaults to "~/Videos/shadowplay.rs/" or "C:/Users/<user>/Videos/shadowplay.rs"
  #[arg(short, long)]
  output: Option<PathBuf>,

  /// Folder to use for temporary files
  #[arg(short = 'm', long)]
  tmp: Option<PathBuf>,

  /// Codec to use when saving
  #[arg(short, long, default_value_t)]
  codec: Codec,

  /// Recording duration in seconds [default: unlimited]
  #[arg(short, long)]
  time: Option<u64>,

  /// Frames per second
  #[arg(short, long)]
  fps: Option<u64>,

  /// Video bitrate in kbps
  #[arg(short, long, default_value_t = 5000)]
  bv: u32,

  /// Audio bitrate in kbps
  #[arg(short = 'a', long, default_value_t = 128)]
  ba: u32,
}

#[derive(Debug, Clone, ValueEnum, Default)]
enum Codec {
  #[default]
  VP8,
  VP9,
}

impl fmt::Display for Codec {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let string = match self {
      Self::VP8 => "vp8",
      Self::VP9 => "vp9",
    };
    write!(f, "{string}")
  }
}

fn main() {
  let args = Cli::parse();

  let max_time = args.time.map(Duration::from_secs);

  let path = args
    .output
    .map_or_else(
      || {
        let home = env::var("HOME").unwrap();
        PathBuf::from(home)
          .join("Videos/shadowplay.rs")
          .canonicalize()
          .expect("Default directory not found")
      },
      PathBuf::from,
    )
    .join("test.webm");

  println!("{path:?}");

  let _tmp = args
    .tmp
    .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);

  let Some(display) = get_display() else { return; };

  // Setup the recorder.
  let mut capturer = Capturer::new(display).expect("Can't initialize capturer");
  let width = capturer.width();
  let height = capturer.height();

  // Setup the multiplexer.
  let Some(out) = get_output_file(&path) else { return; };

  let mut webm =
    mux::Segment::new(mux::Writer::new(out)).expect("Could not initialize the multiplexer.");

  let (vpx_codec, mux_codec) = match args.codec {
    Codec::VP8 => (vpx_encode::VideoCodecId::VP8, mux::VideoCodecId::VP8),
    Codec::VP9 => (vpx_encode::VideoCodecId::VP9, mux::VideoCodecId::VP9),
  };

  let mut video_track = webm.add_video_track(width as u32, height as u32, None, mux_codec);

  // Setup the encoder.
  let mut vpx_encoder = vpx_encode::Encoder::new(vpx_encode::Config {
    width: width as u32,
    height: height as u32,
    timebase: [1, 1000],
    bitrate: args.bv,
    codec: vpx_codec,
  })
  .expect("Can't initialize encoder");

  // Start recording.
  let start = Instant::now();
  let stop = Arc::new(AtomicBool::new(false));

  thread::spawn({
    let stop = stop.clone();
    move || setup_hotkey(stop)
  });

  thread::spawn({
    let stop = stop.clone();
    move || {
      quest::ask("Recording! Press ⏎ to stop.");
      let _ = quest::text();
      stop.store(true, Ordering::Release);
    }
  });

  let seconds_per_frame = args
    .fps
    .map(|fps| Duration::from_nanos(1_000_000_000 / fps));

  while !stop.load(Ordering::Acquire) {
    let now = Instant::now();
    let time = now - start;

    if max_time.is_some_and(|d| time > d) {
      break;
    }

    match capturer.frame() {
      Ok(frame) => {
        process_frame(
          width,
          height,
          &frame,
          &mut vpx_encoder,
          time.as_millis(),
          &mut video_track,
        );
      }
      Err(e) if e.kind() == io::ErrorKind::WouldBlock => {} // Wait.
      Err(e) => {
        println!("{e}");
        break;
      }
    }

    if let Some(spf) = seconds_per_frame {
      let dt = now.elapsed();
      if dt < spf {
        thread::sleep(spf - dt);
      }
    }
  }

  // End things.
  let mut frames = vpx_encoder.finish().expect("Can't finish encoding");
  while let Some(frame) = frames.next().expect("Can't read frame") {
    video_track.add_frame(frame.data, frame.pts as u64 * 1_000_000, frame.key);
  }

  let _ = webm.finalize(None);
}

fn process_frame(
  width: usize,
  height: usize,
  frame: &scrap::Frame,
  vpx_encoder: &mut vpx_encode::Encoder,
  millis: u128,
  video_track: &mut mux::VideoTrack,
) {
  let start = Instant::now();
  let yuv_frame = convert::argb_to_yuv420(width, height, frame);
  // let yuv_frame = convert::argb_to_yuv420_with_subsampling(width, height, frame);
  // let yuv_frame = convert::argb_to_yuv444(width, height, frame);
  let elapsed = start.elapsed();
  println!("{elapsed:?}");

  // add frame to the encoding queue
  let encoded = vpx_encoder
    .encode(
      millis as i64,
      &yuv_frame,
      vpx_encode::vpx_img_fmt::VPX_IMG_FMT_I444,
    )
    .expect("Can't encode frame");

  // if there are any frames done encoding add them to the track
  for encoded_frame in encoded {
    video_track.add_frame(
      encoded_frame.data,
      encoded_frame.pts as u64 * 1_000_000,
      encoded_frame.key,
    );
  }
}

fn setup_hotkey(stop: Arc<AtomicBool>) {
  let mut hk = hotkey::Listener::new();
  hk.register_hotkey(hotkey::modifiers::SHIFT, 0xFFC9 /* F12 */, move || {
    println!("Alt-F12 pressed!");
    stop.store(true, Ordering::Release);
  })
  .unwrap();
  hk.listen();
}

fn get_output_file(path: &PathBuf) -> Option<File> {
  let file = OpenOptions::new().write(true).create_new(true).open(path);

  let out = match file {
    Ok(file) => file,
    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
      let confirm = loop {
        quest::ask("Overwrite the existing file? [y/N] ");
        if let Some(b) = quest::yesno(false).expect("Can't read input") {
          break b;
        }
      };

      if confirm {
        File::create(path).expect("Can't create file")
      } else {
        return None;
      }
    }
    e => e.expect("Error opening file"),
  };

  Some(out)
}

fn get_display() -> Option<Display> {
  let displays = Display::all().expect("Displays couldn't be initialized");

  let i = if displays.is_empty() {
    error("No displays found.");
    return None;
  } else if displays.len() == 1 {
    0
  } else {
    let names: Vec<_> = displays
      .iter()
      .enumerate()
      .map(|(i, display)| format!("Display {} [{}x{}]", i, display.width(), display.height(),))
      .collect();

    quest::ask("Which display?\n");
    let i = quest::choose(Boxes::default(), &names).expect("Can't read input");
    println!();

    i
  };

  displays.into_iter().nth(i)
}

fn error<S: fmt::Display>(s: S) {
  println!("\u{1B}[1;31m{s}\u{1B}[0m");
}
