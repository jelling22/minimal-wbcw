use std::env;

use color_eyre::eyre::{Ok, Report, Result};
use gst::{DebugGraphDetails, EventType, PadProbeType, prelude::*};

#[derive(Debug, Default)]
pub struct DebugSettings {
    pub make_dot: bool,
}

pub struct PipelineWrapper {
    pipeline: gst::Pipeline,
    started: bool,
    debug_params: DebugSettings,
}

#[derive(Debug, PartialEq)]
pub enum BusCommandType {
    Eos,
    Started,
    Ended,
    Error,
}

pub fn attach_eos_probe<P: IsA<gst::Pad>>(pad: &P, msg: &str) {
    let mmsg = msg.to_owned();
    pad.add_probe(PadProbeType::EVENT_BOTH, move |_pad, pad_probe_info| {
    if let Some(event) = pad_probe_info.event() {
        if let EventType::Eos = event.type_() {
            println!("{}", mmsg);
        }
    }
    gst::PadProbeReturn::Ok
    });
}

impl PipelineWrapper {
    pub fn new() -> Result<Self, Report> {
        let debug_params = DebugSettings {
            make_dot: env::var("GST_DEBUG_DUMP_DOT_DIR").is_ok(),
        };

        let pipeline = PipelineWrapper {
            pipeline: gst::Pipeline::new(),
            started: false,
            debug_params,
        };

        //cef src
        let cef_src = gst::ElementFactory::make("cefsrc")
            .property_from_str("url", "http://example.com")
            .property_from_str("do-timestamp", "1")
            .property_from_str("num-buffers", "300")
            .build()?;
        attach_eos_probe(&cef_src.static_pad("src").unwrap(), "! cef emitted eos after 300 buffers");
        //cef -> demux
        let cef_demux = gst::ElementFactory::make("cefdemux").build()?;
        pipeline.pipeline.add_many([&cef_src, &cef_demux])?;
        cef_src.link(&cef_demux)?;

        //VIDEO
        //make video demux queue
        let demux_video_queue = gst::ElementFactory::make("queue").build()?;
        pipeline.pipeline.add(&demux_video_queue)?;

        //demux -> video queue
        let demux_video_src_pad = cef_demux.static_pad("video").unwrap();
        let queue_video_sink_pad = demux_video_queue.static_pad("sink").unwrap();
        demux_video_src_pad.link(&queue_video_sink_pad)?;

        //make video convert
        let caps = gst_video::VideoCapsBuilder::new()
            .width(1280)
            .height(720)
            .framerate((60, 1).into())
            .build();
        let video_capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", &caps)
            .build()?;
        let videoconvert = gst::ElementFactory::make("videoconvert").build()?;
        let videoconvert_caps = gst_video::VideoCapsBuilder::for_encoding("video/x-raw")
            .field("format", "I420")
            .build();
        let videoconvert_capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", &videoconvert_caps)
            .build()?;
        pipeline.pipeline.add_many([&video_capsfilter, &videoconvert, &videoconvert_capsfilter])?;

        //video queue -> video convert
        gst::Element::link_many([
            &demux_video_queue,
            &video_capsfilter,
            &videoconvert,
            &videoconvert_capsfilter
        ])?;

        //make video encoder
        let x264encoder = gst::ElementFactory::make("x264enc")
            .property_from_str("threads", "0")
            .property_from_str("bitrate", "5000")
            .property_from_str("tune", "zerolatency")
            .property_from_str("key-int-max", "30")
            .build()?;
        let encoder_caps = gst_video::VideoCapsBuilder::for_encoding("video/x-h264")
            .field("profile", "main")
            .build();
        let encoder_capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", &encoder_caps)
            .build()?;
        let encode_video_queue = gst::ElementFactory::make("queue").build()?;
        pipeline.pipeline.add_many([&x264encoder, &encoder_capsfilter, &encode_video_queue])?;

        //video convert -> video encoder
        gst::Element::link_many([
            &videoconvert_capsfilter,
            &x264encoder,
            &encoder_capsfilter,
            &encode_video_queue,
        ])?;


        //AUDIO
        //make audio demux queue
        let demux_audio_queue = gst::ElementFactory::make("queue").build()?;
        pipeline.pipeline.add(&demux_audio_queue)?;

        //demux -> cef audio queue
        let demux_audio_src_pad = cef_demux.static_pad("audio").unwrap();
        let queue_audio_sink_pad = demux_audio_queue.static_pad("sink").unwrap();
        demux_audio_src_pad.link(&queue_audio_sink_pad)?;

        //make silence test source
        let silence_audio_src = gst::ElementFactory::make("audiotestsrc")
            .property_from_str("do-timestamp", "true")
            .property_from_str("is-live", "true")
            .property_from_str("volume", "0.00")
            .build()?;
        let silencesrc_caps = gst_audio::AudioCapsBuilder::new()
            .format(gst_audio::AudioFormat::F32le)
            .rate(44100)
            .channels(2)
            .build();
        let silence_capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", silencesrc_caps)
            .name("silence_caps_capsfilter")
            .build()?;
        pipeline.pipeline.add_many([&silence_audio_src, &silence_capsfilter])?;

        //silence source -> caps filter
        silence_audio_src.link(&silence_capsfilter)?;

        //make audio mixer
        let audio_mixer = gst::ElementFactory::make("audiomixer").build()?;
        pipeline.pipeline.add(&audio_mixer)?;
        let mixer_pad_template = audio_mixer.pad_template("sink_%u").unwrap();

        //cef audio queue -> audio mixer
        let cef_audio_queue_src_pad = demux_audio_queue.static_pad("src").unwrap();
        let mixer_cef_sink_pad = audio_mixer.request_pad(&mixer_pad_template, None, None).unwrap();
        cef_audio_queue_src_pad.link(&mixer_cef_sink_pad)?;
        //silent test filter -> audio mixer
        let silence_capsfilter_src_pad = silence_capsfilter.static_pad("src").unwrap();
        let mixer_silence_sink_pad = audio_mixer.request_pad(&mixer_pad_template, None, None).unwrap();
        silence_capsfilter_src_pad.link(&mixer_silence_sink_pad)?;


        //HACKY FIX
        // let mixer_src_pad = audio_mixer.static_pad("src").unwrap();
        // mixer_cef_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        //     // if cef sink pad sees EOS, push EOS to src pad to bypass running silence source
        //     if let Some(gst::PadProbeData::Event(event)) = &info.data {
        //         if let gst::EventView::Eos(_) = event.view() {
        //             let _ = mixer_src_pad.push_event(gst::event::Eos::new());
        //             return gst::PadProbeReturn::Ok;
        //         }
        //     }
        //     gst::PadProbeReturn::Ok
        // });
        //HACKY FIX

        //make audio encode
        let encode_audio = gst::ElementFactory::make("avenc_aac").build()?;
        let encode_audio_queue = gst::ElementFactory::make("queue").build()?;
        pipeline.pipeline.add_many([&encode_audio, &encode_audio_queue])?;
        //audio mixer -> encoding
        gst::Element::link_many([&audio_mixer, &encode_audio, &encode_audio_queue])?;

        //OUTPUT
        //make mp4 mux
        let mux = gst::ElementFactory::make("mp4mux").build()?;
        pipeline.pipeline.add(&mux)?;
        let mux_video_sink_template = mux.pad_template("video_%u").unwrap();
        let mux_audio_sink_template = mux.pad_template("audio_%u").unwrap();

        //video -> mp4 mux
        let mux_video_sink_pad = mux.request_pad(&mux_video_sink_template, None, None).unwrap();
        let encode_video_src_pad = encode_video_queue.static_pad("src").unwrap();
        encode_video_src_pad.link(&mux_video_sink_pad)?;
        //audio -> mp4 mux
        let mux_audio_sink_pad = mux.request_pad(&mux_audio_sink_template, None, None).unwrap();
        let encode_audio_src_pad = encode_audio_queue.static_pad("src").unwrap();
        encode_audio_src_pad.link(&mux_audio_sink_pad)?;

        //make filesink
        let sink = gst::ElementFactory::make("filesink").property("location", "pipeline-test.mp4").build()?;
        pipeline.pipeline.add(&sink)?;
        mux.link(&sink)?;

        Ok(pipeline)
    }

    pub fn play(&self) -> Result<(), Report> {
        self.pipeline.set_state(gst::State::Playing)?;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), Report> {
        println!("sending eos");
        self.pipeline.send_event(gst::event::Eos::new());
        Ok(())
    }

    pub fn handle_pipeline_message(&mut self) -> Option<BusCommandType> {
        use gst::message::MessageView;

        if let Some(Some(message)) = self.pipeline.bus().map(|bus| bus.pop()) {
            match message.view() {
                MessageView::Error(err) => {
                    println!(
                        "Error from element {}: {} ({})",
                        err.src()
                            .map(|s| String::from(s.path_string()))
                            .unwrap_or_else(|| String::from("None")),
                        err.error(),
                        err.debug().unwrap_or_else(|| glib::GString::from("None")),
                    );
                    return Some(BusCommandType::Error);
                }
                MessageView::Warning(warning) => {
                    println!("Warning: \"{}\"", warning.debug().unwrap());
                }
                MessageView::Latency(_) => {
                    let _ = self.pipeline.recalculate_latency();
                }
                MessageView::StateChanged(state) => {
                    if let Some(obj) = state.src() {
                        if obj.is::<gst::Pipeline>() && self.debug_params.make_dot {
                            self.pipeline.debug_to_dot_file(
                                DebugGraphDetails::ALL,
                                format!("{:?}", state.current()),
                            );
                        }
                    }
                    if state.current() == gst::State::Null && self.started {
                        println!("pipeline ended");
                        self.started = false;
                        return Some(BusCommandType::Ended);
                    } else if state.current() == gst::State::Playing && !self.started {
                        println!("pipeline started");
                        self.started = true;
                        return Some(BusCommandType::Started);
                    }
                }
                MessageView::Eos(_) => {
                    println!("saw EOS, setting pipeline -> NULL");
                    if self.pipeline.set_state(gst::State::Null).is_ok() {
                        return Some(BusCommandType::Eos);
                    } else {
                        println!("Error setting NULL on pipeline");
                    }
                }
                _ => (),
            }
        }

        None
    }

}
