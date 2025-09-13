slint::include_modules!();

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use slint::winit_030::{WinitWindowAccessor, winit};
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

const APP_NAME: &str = "Simple TTS Reader";
const APP_ID: &str = "DmitryMaluev.SimpleTTSReader";

pub fn get_log_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut path = confy::get_configuration_file_path(APP_NAME, "log")?;
    path.pop();
    path.pop();
    path.push("log.txt");
    Ok(path)
}

fn get_dictionary_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut path = confy::get_configuration_file_path(APP_NAME, "dictionary")?;
    path.set_extension("txt");
    Ok(path)
}

fn get_recordings_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut path = confy::get_configuration_file_path(APP_NAME, "recordings")?;
    path.pop();
    path.pop();
    path.push("recordings");
    let _ = std::fs::create_dir(&path);
    Ok(path)
}

fn get_recording_file_path(ext: &str, index: Option<u32>) -> Result<PathBuf, Box<dyn Error>> {
    let mut path = get_recordings_path()?;
    let local_datetime = chrono::Local::now();
    let mut timestamp = local_datetime.format("%Y%m%d_%H%M%S").to_string();
    if let Some(index) = index {
        timestamp.push_str(&format!("({index})"));
    }
    path.push(timestamp);
    path.set_extension(ext);
    Ok(path)
}

fn get_valid_recording_file_path(ext: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = get_recording_file_path(ext, None)?;
    // Try without index:
    if std::fs::exists(&path)? {
        // Find suitable index:
        let mut index: u32 = 0;
        loop {
            let path = get_recording_file_path(ext, Some(index))?;
            if !std::fs::exists(&path)? {
                return Ok(path);
            }
            index += 1;
            if index == 100 {
                return Err("Too many indices".into());
            }
        }
    } else {
        Ok(path)
    }
}

pub fn open_recordings_path() -> Result<(), Box<dyn Error>> {
    let path = get_recordings_path()?;
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()?
        .wait()?;
    Ok(())
}

fn center_winit_window(window: &winit::window::Window) {
    if let Some(monitor) = window.current_monitor() {
        let mon_size = monitor.size();
        let wnd_size = window.outer_size();
        window.set_outer_position(winit::dpi::PhysicalPosition {
            x: mon_size.width.saturating_sub(wnd_size.width) as f64 * 0.5
                + monitor.position().x as f64,
            y: mon_size.height.saturating_sub(wnd_size.height) as f64 * 0.5
                + monitor.position().y as f64,
        });
    }
}

fn center_window(weak_main_window: slint::Weak<MainWindow>, show: bool) {
    weak_main_window
        .upgrade_in_event_loop(move |main_window| {
            main_window.window().with_winit_window(center_winit_window);
            if show {
                main_window.show().unwrap();
            }
        })
        .unwrap();
}

#[derive(Serialize, Deserialize, PartialEq, Copy, Clone)]
enum OutputKind {
    Speak,
    SaveWAV,
    SaveOpus,
    SaveOggVorbis,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct Config {
    voice_name: String,
    rate: i32,
    volume: u32,
    hidden: bool,
    output: OutputKind,
    hq_audio: bool,
}

impl Config {
    fn load(sanitize: bool) -> Self {
        if let Ok(mut config) = confy::load::<Self>(APP_NAME, "config") {
            if sanitize {
                config.rate = config.rate.clamp(-10, 10);
                config.volume = config.volume.clamp(0, 100);
            }
            config
        } else {
            Self::default()
        }
    }

    fn store(&self) {
        let _ = confy::store(APP_NAME, "config", self);
    }
}

impl std::default::Default for Config {
    fn default() -> Self {
        Self {
            voice_name: String::from(""),
            rate: 0,
            volume: 100,
            hidden: false,
            output: OutputKind::Speak,
            hq_audio: false,
        }
    }
}

struct Dictionary {
    word_map: HashMap<String, String>,
}

impl Dictionary {
    fn build() -> Result<Self, Box<dyn Error>> {
        let mut dictionary = Dictionary {
            word_map: HashMap::new(),
        };
        let path = get_dictionary_path()?;
        dictionary.load(&path)?;
        Ok(dictionary)
    }

    fn replace_words(&self, input: &str) -> Option<String> {
        if self.word_map.is_empty() {
            return None;
        }

        let is_separator = |c: char| c.is_ascii_whitespace() || c.is_ascii_punctuation();

        let mut result = String::with_capacity(input.len());
        for token in input.split_inclusive(is_separator) {
            let old_word: String;
            let separator: char;
            if let Some(last_char) = token.chars().last() {
                if is_separator(last_char) {
                    old_word = token[0..token.len() - 1].to_lowercase();
                    separator = last_char;
                } else {
                    old_word = token.to_lowercase();
                    separator = '\0';
                }
            } else {
                old_word = token.to_lowercase();
                separator = '\0';
            }

            if let Some(new_word) = self.word_map.get(&old_word) {
                result.push_str(new_word);
                if separator != '\0' {
                    result.push(separator);
                }
            } else {
                result.push_str(token);
            }
        }
        Some(result)
    }

    fn load(&mut self, path: &Path) -> std::io::Result<()> {
        self.word_map.clear();
        if std::fs::exists(path)? {
            let contents = std::fs::read_to_string(path)?;
            for line in contents.lines() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some((key, value)) = line.split_once('=') {
                    self.word_map
                        .insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
        Ok(())
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        let mut v: Vec<String> = self
            .word_map
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        v.sort_unstable();
        for line in v {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    fn get_model(&self) -> slint::ModelRc<slint::StandardListViewItem> {
        let mut v: Vec<slint::StandardListViewItem> = self
            .word_map
            .iter()
            .map(|(key, value)| {
                slint::StandardListViewItem::from(format!("{}={}", key, value).as_str())
            })
            .collect();
        v.sort_unstable_by(|a, b| a.text.cmp(&b.text));
        slint::ModelRc::new(slint::VecModel::<slint::StandardListViewItem>::from(v))
    }
}

fn get_voice_name(voice: &sapi_lite::tts::Voice) -> String {
    let name = voice
        .name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let lang = voice
        .language()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if lang.is_empty() {
        name
    } else {
        format!("{name} [{lang}]")
    }
}

struct SapiEventHandler {
    weak_speech_app: Option<Weak<Mutex<SpeechApp>>>,
    weak_cv_finished: Option<Weak<Condvar>>,
}

impl SapiEventHandler {
    fn new() -> Self {
        SapiEventHandler {
            weak_speech_app: None,
            weak_cv_finished: None,
        }
    }
}

impl sapi_lite::tts::EventHandler for SapiEventHandler {
    fn on_speech_finished(&self, _id: u32) {
        if let Some(weak_speech_app) = self.weak_speech_app.as_ref()
            && let Some(speech_app) = Weak::upgrade(weak_speech_app)
        {
            speech_app.lock().save_audio().unwrap();
        }

        if let Some(weak_cv_finished) = self.weak_cv_finished.as_ref()
            && let Some(cv_finished) = Weak::upgrade(weak_cv_finished)
        {
            cv_finished.notify_all();
        }
    }
}

struct SpeechApp {
    synth: sapi_lite::tts::EventfulSynthesizer,
    voices: Vec<sapi_lite::tts::Voice>,
    memory_stream: sapi_lite::audio::MemoryStream,
    config: Config,
    dictionary: Dictionary,
    toast_manager: winrt_toast::ToastManager,

    // For SapiEventHandler:
    weak_speech_app: Option<Weak<Mutex<SpeechApp>>>,
    weak_cv_finished: Option<Weak<Condvar>>,

    // Windows:
    weak_about_window: Option<slint::Weak<AboutWindow>>,
    weak_dictionary_window: Option<slint::Weak<DictionaryWindow>>,
}

impl SpeechApp {
    fn build(config: Config, dictionary: Dictionary) -> Result<Self, Box<dyn Error>> {
        sapi_lite::initialize()?;

        let mut speech_app = Self {
            synth: sapi_lite::tts::EventfulSynthesizer::new(SapiEventHandler::new())?,
            voices: sapi_lite::tts::installed_voices(None, None)?.collect(),
            memory_stream: sapi_lite::audio::MemoryStream::new(None)?,
            config,
            dictionary,
            toast_manager: winrt_toast::ToastManager::new(APP_ID),

            weak_speech_app: None,
            weak_cv_finished: None,

            weak_about_window: None,
            weak_dictionary_window: None,
        };

        speech_app.set_voice(None)?;
        speech_app.set_rate(None)?;
        speech_app.set_volume(None)?;

        Ok(speech_app)
    }

    fn get_voice_by_name(&self, voice_name: Option<&str>) -> Option<&sapi_lite::tts::Voice> {
        let voice_name = voice_name.unwrap_or(&self.config.voice_name);
        self.voices
            .iter()
            .find(|voice| get_voice_name(voice) == voice_name)
    }

    fn get_voice_name_by_index(&self, index: usize) -> String {
        if index < self.voices.len() {
            get_voice_name(&self.voices[index])
        } else {
            String::new()
        }
    }

    fn get_voice_index(&self, voice_name: Option<&str>) -> Option<usize> {
        let voice_name = voice_name.unwrap_or(&self.config.voice_name);
        self.voices
            .iter()
            .position(|voice| get_voice_name(voice) == voice_name)
    }

    fn set_voice(&mut self, voice_name: Option<&str>) -> Result<(), Box<dyn Error>> {
        if let Some(voice_name) = voice_name {
            self.config.voice_name = String::from(voice_name);
            self.config.store();
        }

        if let Some(voice) = self.get_voice_by_name(voice_name) {
            self.synth.set_voice(voice)?;
        } else if !self.voices.is_empty() {
            self.synth.set_voice(&self.voices[0])?;
        }
        Ok(())
    }

    fn set_rate(&mut self, rate: Option<i32>) -> Result<(), Box<dyn Error>> {
        if let Some(rate) = rate {
            self.config.rate = rate;
            self.config.store();
        }

        Ok(self.synth.set_rate(self.config.rate)?)
    }

    fn set_volume(&mut self, volume: Option<u32>) -> Result<(), Box<dyn Error>> {
        if let Some(volume) = volume {
            self.config.volume = volume;
            self.config.store();
        }

        Ok(self.synth.set_volume(self.config.volume)?)
    }

    fn set_output(&mut self, output: OutputKind) {
        if self.config.output != output {
            self.config.output = output;
            self.config.store();
        }
    }

    fn speak(&mut self, speech: &str) -> Result<u32, Box<dyn Error>> {
        // TODO: Find a better way to stop active speech
        self.synth = sapi_lite::tts::EventfulSynthesizer::new(SapiEventHandler {
            weak_speech_app: self.weak_speech_app.as_ref().map(Weak::clone),
            weak_cv_finished: self.weak_cv_finished.as_ref().map(Weak::clone),
        })?;

        self.set_voice(None)?;
        self.set_rate(None)?;
        self.set_volume(None)?;

        if self.config.output != OutputKind::Speak {
            let audio_format = sapi_lite::audio::AudioFormat {
                sample_rate: if self.config.hq_audio {
                    sapi_lite::audio::SampleRate::Hz48000
                } else {
                    sapi_lite::audio::SampleRate::Hz16000
                },
                bit_rate: sapi_lite::audio::BitRate::Bits16,
                channels: sapi_lite::audio::Channels::Mono,
            };
            let audio_stream = sapi_lite::audio::AudioStream::from_stream(
                self.memory_stream.try_clone().unwrap(),
                &audio_format,
            )?;
            self.synth
                .set_output(sapi_lite::tts::SpeechOutput::Stream(audio_stream), false)?;
        }

        let new_speech = self.dictionary.replace_words(speech);

        Ok(self.synth.speak(new_speech.as_deref().unwrap_or(speech))?)
    }

    fn save_audio(&self) -> Result<(), Box<dyn Error>> {
        if self.config.output == OutputKind::Speak {
            return Ok(());
        }

        let stream = windows::Win32::System::Com::IStream::from(self.memory_stream.try_clone()?);

        let mut data: Vec<u8> = Vec::with_capacity(0x1000);
        let mut samples: Vec<i16> = Vec::with_capacity(0x8000);
        let mut chunk = [0u8; 0x1000];
        let mut cbread: u32 = 0;
        loop {
            unsafe {
                let pv = &mut chunk[0] as *mut u8 as *mut core::ffi::c_void;
                let pcbread = &mut cbread as *mut u32;
                stream.Read(pv, chunk.len() as u32, pcbread)?;
            }
            if cbread == 0 {
                break;
            }

            data.extend_from_slice(&chunk[..cbread as usize]);
            let mut drain_count = 0;
            for sample in data
                .chunks_exact(2)
                .map(|x| i16::from_le_bytes(x.try_into().unwrap()))
            {
                samples.push(sample);
                drain_count += 2;
            }
            data.drain(0..drain_count);
        }

        unsafe {
            stream.SetSize(0)?;
        }
        if samples.is_empty() {
            return Ok(());
        }

        let sample_rate = if self.config.hq_audio { 48000 } else { 16000 };
        let mut notif = String::from("");
        match self.config.output {
            OutputKind::SaveWAV => {
                let path = get_valid_recording_file_path("wav")?;
                let wav_spec = hound::WavSpec {
                    channels: 1,
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut wav_writer = hound::WavWriter::create(path, wav_spec)?;
                for sample in &samples {
                    wav_writer.write_sample(*sample)?;
                }
                wav_writer.finalize()?;
                notif = String::from("WAV file is ready");
            }
            OutputKind::SaveOpus => {
                let path = get_valid_recording_file_path("opus")?;
                let mut file = std::fs::File::create(path)?;
                let encoded = if self.config.hq_audio {
                    ogg_opus::encode::<48000, 1>(&samples)?
                } else {
                    ogg_opus::encode::<16000, 1>(&samples)?
                };
                file.write_all(&encoded)?;
                notif = String::from("Opus file is ready");
            }
            OutputKind::SaveOggVorbis => {
                let path = get_valid_recording_file_path("ogg")?;
                let mut file = std::fs::File::create(path)?;
                let mut encoder = vorbis_encoder::Encoder::new(
                    1,
                    sample_rate as u64,
                    if self.config.hq_audio { 0.5 } else { 0.2 },
                )
                .unwrap();
                let encoded = encoder.encode(&samples).unwrap();
                file.write_all(&encoded)?;
                let encoded = encoder.flush().unwrap();
                file.write_all(&encoded)?;
                notif = String::from("Ogg Vorbis file is ready");
            }
            _ => (),
        }

        if !notif.is_empty() {
            if self.config.hq_audio {
                notif += " (HQ)";
            }
            let mut toast = winrt_toast::Toast::new();
            toast
                .text1(notif)
                .expires_in(std::time::Duration::from_secs(60 * 60));
            self.toast_manager.show(&toast).unwrap();
        }

        Ok(())
    }

    fn hide_about_window(&self) {
        if let Some(window) = self.weak_about_window.as_ref() {
            window.upgrade().inspect(|w| w.hide().unwrap());
        }
    }

    fn hide_dictionary_window(&self) {
        if let Some(window) = self.weak_dictionary_window.as_ref() {
            window.upgrade().inspect(|w| w.hide().unwrap());
        }
    }
}

impl Drop for SpeechApp {
    fn drop(&mut self) {
        sapi_lite::finalize();
    }
}

struct ClipboardListener {
    clipboard: arboard::Clipboard,
    weak_speech_app: Weak<Mutex<SpeechApp>>,
}

impl ClipboardListener {
    fn spawn(weak_speech_app: Weak<Mutex<SpeechApp>>) {
        std::thread::spawn(move || {
            let Ok(clipboard) = arboard::Clipboard::new() else {
                return;
            };

            let listener = ClipboardListener {
                clipboard,
                weak_speech_app,
            };

            let _ = clipboard_master::Master::new(listener).run();
        });
    }
}

impl clipboard_master::ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> clipboard_master::CallbackResult {
        if let Ok(text) = self.clipboard.get_text()
            && let Some(speech_app) = Weak::upgrade(&self.weak_speech_app)
        {
            let _ = speech_app.lock().speak(&text);
        }
        clipboard_master::CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, _error: std::io::Error) -> clipboard_master::CallbackResult {
        clipboard_master::CallbackResult::Next
    }
}

pub fn run(hidden: Option<bool>) -> Result<(), Box<dyn Error>> {
    let mut config;
    {
        let original_config = Config::load(false);
        config = Config::load(true);

        config.hidden = hidden.unwrap_or(config.hidden);

        if config != original_config {
            config.store();
        }
    }

    let dictionary = Dictionary::build()?;

    let speech_app = Arc::new(Mutex::new(SpeechApp::build(config, dictionary)?));
    speech_app.lock().weak_speech_app = Some(Arc::downgrade(&speech_app));

    ClipboardListener::spawn(Arc::downgrade(&speech_app));

    let main_window = MainWindow::new()?;
    main_window.set_app_name(slint::SharedString::from(APP_NAME));

    let _tray_icon;
    {
        let weak_main_window = main_window.as_weak();
        let icon = tray_icon::Icon::from_resource_name("app_icon", None)?;
        _tray_icon = tray_icon::TrayIconBuilder::new()
            .with_tooltip(APP_NAME)
            .with_icon(icon)
            .build()
            .unwrap();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                weak_main_window
                    .upgrade_in_event_loop(move |main_window| {
                        main_window
                            .window()
                            .with_winit_window(|window: &winit::window::Window| {
                                let is_vis = main_window.window().is_visible();
                                let is_min = main_window.window().is_minimized();
                                if is_vis && !is_min {
                                    main_window.hide().unwrap();
                                } else {
                                    main_window.window().set_minimized(false);
                                    main_window.show().unwrap();
                                    window.focus_window();
                                }
                            });
                    })
                    .unwrap();
            }
        }));
    }

    {
        let v: Vec<slint::StandardListViewItem> = speech_app
            .lock()
            .voices
            .iter()
            .map(|voice| slint::StandardListViewItem::from(get_voice_name(voice).as_str()))
            .collect();
        let model = slint::ModelRc::new(slint::VecModel::<slint::StandardListViewItem>::from(v));
        main_window.set_voices_list_model(model);

        let index = speech_app.lock().get_voice_index(None).unwrap_or(0);
        main_window.invoke_voices_list_set_current_item(index as i32);
    }

    main_window.set_rate(speech_app.lock().config.rate as f32);
    main_window.set_volume(speech_app.lock().config.volume as f32);
    main_window.set_output_cb_current_index(speech_app.lock().config.output as i32);

    main_window.window().on_close_requested(|| {
        slint::quit_event_loop().unwrap();
        slint::CloseRequestResponse::HideWindow
    });

    main_window.on_voices_list_current_item_changed({
        let speech_app = Arc::clone(&speech_app);
        move |index: i32| {
            let name = speech_app.lock().get_voice_name_by_index(index as usize);
            speech_app.lock().set_voice(Some(&name)).unwrap();
        }
    });
    main_window.on_rate_slider_released({
        let speech_app = Arc::clone(&speech_app);
        move |position: f32| {
            let rate = position.round() as i32;
            speech_app.lock().set_rate(Some(rate)).unwrap();
        }
    });
    main_window.on_volume_slider_released({
        let speech_app = Arc::clone(&speech_app);
        move |position: f32| {
            let volume = position.round() as u32;
            speech_app.lock().set_volume(Some(volume)).unwrap();
        }
    });
    main_window.on_about_button_clicked({
        let speech_app = Arc::clone(&speech_app);
        move || {
            speech_app.lock().hide_about_window();

            let version = format!(" v{}", env!("CARGO_PKG_VERSION"));
            let about_window = AboutWindow::new().unwrap();
            about_window.set_app_name(slint::SharedString::from(APP_NAME));
            about_window.set_app_version(slint::SharedString::from(version));
            about_window.set_hidden(speech_app.lock().config.hidden);
            about_window.set_hq_audio(speech_app.lock().config.hq_audio);

            about_window.on_hidden_cb_toggled({
                let weak_about_window = about_window.as_weak();
                let speech_app = Arc::clone(&speech_app);
                move || {
                    let about_window = weak_about_window.unwrap();
                    speech_app.lock().config.hidden = about_window.get_hidden();
                    speech_app.lock().config.store();
                }
            });
            about_window.on_hq_audio_cb_toggled({
                let weak_about_window = about_window.as_weak();
                let speech_app = Arc::clone(&speech_app);
                move || {
                    let about_window = weak_about_window.unwrap();
                    speech_app.lock().config.hq_audio = about_window.get_hq_audio();
                    speech_app.lock().config.store();
                }
            });

            speech_app.lock().weak_about_window = Some(about_window.as_weak());
            about_window.show().unwrap();
        }
    });
    main_window.on_test_button_clicked({
        let weak_main_window = main_window.as_weak();
        let speech_app = Arc::clone(&speech_app);
        move || {
            let main_window = weak_main_window.unwrap();
            speech_app
                .lock()
                .speak(&main_window.get_test_string())
                .unwrap();
        }
    });
    main_window.on_dict_button_clicked({
        let speech_app = Arc::clone(&speech_app);
        move || {
            speech_app.lock().hide_dictionary_window();

            let dictionary_window = DictionaryWindow::new().unwrap();

            dictionary_window.set_word_list_model(speech_app.lock().dictionary.get_model());

            dictionary_window.on_add_button_clicked({
                let weak_dictionary_window = dictionary_window.as_weak();
                let speech_app = Arc::clone(&speech_app);
                move || {
                    let dictionary_window = weak_dictionary_window.unwrap();

                    let old_word: String = dictionary_window
                        .get_old_word_string()
                        .trim()
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect();
                    let new_word: String = dictionary_window
                        .get_new_word_string()
                        .trim()
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect();
                    if !old_word.is_empty() && !new_word.is_empty() {
                        speech_app
                            .lock()
                            .dictionary
                            .word_map
                            .insert(old_word, new_word);

                        if let Ok(path) = get_dictionary_path() {
                            let _ = speech_app.lock().dictionary.save(&path);
                        }
                        dictionary_window
                            .set_word_list_model(speech_app.lock().dictionary.get_model());
                    }
                }
            });
            dictionary_window.on_delete_button_clicked({
                let weak_dictionary_window = dictionary_window.as_weak();
                let speech_app = Arc::clone(&speech_app);
                move || {
                    let dictionary_window = weak_dictionary_window.unwrap();

                    let item = String::from(dictionary_window.invoke_word_list_get_current_item());
                    if !item.is_empty()
                        && let Some((key, _)) = item.split_once('=')
                    {
                        speech_app.lock().dictionary.word_map.remove(key);

                        if let Ok(path) = get_dictionary_path() {
                            let _ = speech_app.lock().dictionary.save(&path);
                        }
                        dictionary_window
                            .set_word_list_model(speech_app.lock().dictionary.get_model());
                    }
                }
            });

            speech_app.lock().weak_dictionary_window = Some(dictionary_window.as_weak());
            dictionary_window.show().unwrap();
        }
    });
    main_window.on_output_cb_selected({
        let weak_main_window = main_window.as_weak();
        let speech_app = Arc::clone(&speech_app);
        move |_value: slint::SharedString| {
            let main_window = weak_main_window.unwrap();
            let index = main_window.get_output_cb_current_index();
            match index {
                0 => speech_app.lock().set_output(OutputKind::Speak),
                1 => speech_app.lock().set_output(OutputKind::SaveWAV),
                2 => speech_app.lock().set_output(OutputKind::SaveOpus),
                3 => speech_app.lock().set_output(OutputKind::SaveOggVorbis),
                _ => (),
            }
        }
    });
    main_window.on_output_button_clicked({
        move || {
            let _ = open_recordings_path();
        }
    });
    main_window.on_link_ta_clicked({
        move || {
            let _ = open::that("https://simplettsreader.sourceforge.io/");
        }
    });

    center_window(main_window.as_weak(), !speech_app.lock().config.hidden);
    slint::run_event_loop_until_quit()?;
    main_window.hide()?;

    Ok(())
}

pub fn run_simple(text: Option<String>, path: Option<String>) -> Result<(), Box<dyn Error>> {
    let final_text = match (text, path) {
        (Some(t), _) => t,
        (None, Some(p)) => std::fs::read_to_string(p)?,
        _ => String::new(),
    };

    if final_text.is_empty() {
        return Ok(());
    }

    let config = Config::load(true);
    let dictionary = Dictionary::build()?;

    let speech_app = Arc::new(Mutex::new(SpeechApp::build(config, dictionary)?));
    let cv_finished = Arc::new(Condvar::new());
    speech_app.lock().weak_cv_finished = Some(Arc::downgrade(&cv_finished));

    {
        let mut mutex_guard = speech_app.lock();
        mutex_guard.speak(&final_text)?;
        cv_finished.wait(&mut mutex_guard);
    }

    Ok(())
}
