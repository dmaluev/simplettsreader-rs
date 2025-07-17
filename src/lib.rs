slint::include_modules!();

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

const APP_NAME: &str = "Simple TTS Reader";

fn get_dictionary_path() -> Option<PathBuf> {
    if let Ok(mut path) = confy::get_configuration_file_path(APP_NAME, "dictionary") {
        path.set_extension("txt");
        Some(path)
    } else {
        None
    }
}

fn get_recordings_path() -> Option<PathBuf> {
    if let Ok(mut path) = confy::get_configuration_file_path(APP_NAME, "recordings") {
        path.pop();
        path.pop();
        path.push("recordings");
        std::fs::create_dir(&path).ok();
        Some(path)
    } else {
        None
    }
}

fn get_recording_file_path(ext: &str, index: Option<u32>) -> Option<PathBuf> {
    if let Some(mut path) = get_recordings_path() {
        let local_datetime = chrono::Local::now();
        let mut timestamp = local_datetime.format("%Y%m%d_%H%M%S").to_string();
        if let Some(index) = index {
            timestamp.push_str(&format!("({index})"));
        }
        path.push(timestamp);
        path.set_extension(ext);
        Some(path)
    } else {
        None
    }
}

fn get_valid_recording_file_path(ext: &str) -> Option<PathBuf> {
    if let Some(path) = get_recording_file_path(ext, None) {
        // Try without index:
        if std::fs::exists(&path).unwrap() {
            // Find suitable index:
            let mut index: u32 = 0;
            loop {
                if let Some(path) = get_recording_file_path(ext, Some(index)) {
                    if !std::fs::exists(&path).unwrap() {
                        return Some(path);
                    }
                }
                index += 1;
                if index == 100 {
                    return None; // Too many indices.
                }
            }
        } else {
            Some(path)
        }
    } else {
        None
    }
}

#[derive(Serialize, Deserialize, PartialEq, Copy, Clone)]
enum OutputKind {
    Speak,
    SaveWAV,
    SaveOpus,
    SaveOggVorbis,
}

#[derive(Serialize, Deserialize, PartialEq)]
struct Config {
    voice_name: String,
    rate: i32,
    volume: u32,
    hidden: bool,
    output: OutputKind,
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
        confy::store(APP_NAME, "config", self).ok();
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
        }
    }
}

struct Dictionary {
    word_map: HashMap<String, String>,
}

impl Dictionary {
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
                    old_word = (&token[0..token.len() - 1]).to_lowercase();
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

    fn load(&mut self, path: &Path) {
        self.word_map.clear();
        if let Ok(contents) = std::fs::read_to_string(path) {
            for line in contents.lines() {
                if let Some(kv) = line.split_once('=') {
                    self.word_map.insert(String::from(kv.0), String::from(kv.1));
                }
            }
        }
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        for (key, value) in self.word_map.iter() {
            writeln!(&mut file, "{key}={value}")?;
        }
        Ok(())
    }

    fn get_model(&self) -> slint::ModelRc<slint::StandardListViewItem> {
        let v: Vec<slint::StandardListViewItem> = self
            .word_map
            .iter()
            .map(|kv| slint::StandardListViewItem::from(format!("{}={}", kv.0, kv.1).as_str()))
            .collect();
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
    format!("{name} [{lang}]")
}

struct SapiEventHandler {
    weak_speech_app: Option<Weak<Mutex<SpeechApp>>>,
}

impl sapi_lite::tts::EventHandler for SapiEventHandler {
    fn on_speech_finished(&self, _id: u32) {
        if let Some(weak_speech_app) = self.weak_speech_app.as_ref() {
            if let Some(speech_app) = Weak::upgrade(weak_speech_app) {
                speech_app.lock().save_audio().unwrap();
            }
        }
    }
}

struct SpeechApp {
    synth: sapi_lite::tts::EventfulSynthesizer,
    voices: Vec<sapi_lite::tts::Voice>,
    memory_stream: sapi_lite::audio::MemoryStream,
    config: Config,
    dictionary: Dictionary,
    weak_speech_app: Option<Weak<Mutex<SpeechApp>>>,
}

impl SpeechApp {
    fn build(config: Config, dictionary: Dictionary) -> Result<Self, Box<dyn Error>> {
        sapi_lite::initialize()?;

        let mut speech_app = Self {
            synth: sapi_lite::tts::EventfulSynthesizer::new(SapiEventHandler {
                weak_speech_app: None,
            })?,
            voices: sapi_lite::tts::installed_voices(None, None)?.collect(),
            memory_stream: sapi_lite::audio::MemoryStream::new(None)?,
            config,
            dictionary,
            weak_speech_app: None,
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
            weak_speech_app: if self.weak_speech_app.is_some() {
                Some(self.weak_speech_app.as_ref().unwrap().clone())
            } else {
                None
            },
        })?;

        self.set_voice(None)?;
        self.set_rate(None)?;
        self.set_volume(None)?;

        if self.config.output != OutputKind::Speak {
            let audio_format = sapi_lite::audio::AudioFormat {
                sample_rate: sapi_lite::audio::SampleRate::Hz16000,
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

        let mut data: Vec<u8> = vec![];
        let mut chunk = [0 as u8; 4096];
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
            data.extend(&chunk[0..cbread as usize]);
        }
        let samples = unsafe { data.align_to::<i16>().1 };

        unsafe {
            stream.SetSize(0)?;
        }
        if data.is_empty() {
            return Ok(());
        }

        match self.config.output {
            OutputKind::SaveWAV => {
                if let Some(path) = get_valid_recording_file_path("wav") {
                    let wav_spec = hound::WavSpec {
                        channels: 1,
                        sample_rate: 16000,
                        bits_per_sample: 16,
                        sample_format: hound::SampleFormat::Int,
                    };
                    let mut wav_writer = hound::WavWriter::create(path, wav_spec)?;
                    for sample in samples.iter() {
                        wav_writer.write_sample(*sample)?;
                    }
                    wav_writer.finalize()?;
                }
            }
            OutputKind::SaveOpus => {
                if let Some(path) = get_valid_recording_file_path("opus") {
                    if let Ok(mut file) = std::fs::File::create(path) {
                        let encoded = ogg_opus::encode::<16000, 1>(&samples)?;
                        file.write_all(&encoded)?;
                    }
                }
            }
            OutputKind::SaveOggVorbis => {
                if let Some(path) = get_valid_recording_file_path("ogg") {
                    if let Ok(mut file) = std::fs::File::create(path) {
                        let mut encoder = vorbis_encoder::Encoder::new(1, 16000, 0.2).unwrap();
                        let encoded = encoder.encode(&samples.to_vec()).unwrap();
                        file.write_all(&encoded)?;
                        let encoded = encoder.flush().unwrap();
                        file.write_all(&encoded)?;
                    }
                }
            }
            _ => (),
        }

        Ok(())
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

            clipboard_master::Master::new(listener).run().ok();
        });
    }
}

impl clipboard_master::ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> clipboard_master::CallbackResult {
        if let Ok(text) = self.clipboard.get_text() {
            if let Some(speech_app) = Weak::upgrade(&self.weak_speech_app) {
                speech_app.lock().speak(&text).ok();
            }
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

        if let Some(hidden) = hidden {
            config.hidden = hidden;
        }

        if config != original_config {
            config.store();
        }
    }

    let mut dictionary = Dictionary {
        word_map: HashMap::new(),
    };
    if let Some(path) = get_dictionary_path() {
        dictionary.load(&path);
    }

    let speech_app = Arc::new(Mutex::new(SpeechApp::build(config, dictionary)?));
    speech_app.lock().weak_speech_app = Some(Arc::downgrade(&speech_app));

    ClipboardListener::spawn(Arc::downgrade(&speech_app));

    let app_window = AppWindow::new()?;
    app_window.set_app_name(slint::SharedString::from(APP_NAME));

    let _tray_icon;
    {
        let weak_app_window = app_window.as_weak();
        let icon = tray_icon::Icon::from_resource_name("app-icon", None)?;
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
                weak_app_window
                    .upgrade_in_event_loop(move |app_window| {
                        if app_window.window().is_visible() {
                            app_window.hide().unwrap();
                        } else {
                            app_window.show().unwrap();
                        }
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
        app_window.set_voices_list_model(model);

        let index = speech_app.lock().get_voice_index(None).unwrap_or(0);
        app_window.invoke_voices_list_set_current_item(index as i32);
    }

    app_window.set_rate(speech_app.lock().config.rate as f32);
    app_window.set_volume(speech_app.lock().config.volume as f32);
    app_window.set_output_cb_current_index(speech_app.lock().config.output as i32);

    app_window.window().on_close_requested(|| {
        slint::quit_event_loop().unwrap();
        slint::CloseRequestResponse::HideWindow
    });

    app_window.on_voices_list_current_item_changed({
        let speech_app = speech_app.clone();
        move |index: i32| {
            let name = speech_app.lock().get_voice_name_by_index(index as usize);
            speech_app.lock().set_voice(Some(&name)).unwrap();
        }
    });
    app_window.on_rate_slider_released({
        let speech_app = speech_app.clone();
        move |position: f32| {
            let rate = position.round() as i32;
            speech_app.lock().set_rate(Some(rate)).unwrap();
        }
    });
    app_window.on_volume_slider_released({
        let speech_app = speech_app.clone();
        move |position: f32| {
            let volume = position.round() as u32;
            speech_app.lock().set_volume(Some(volume)).unwrap();
        }
    });
    app_window.on_about_button_clicked({
        let speech_app = speech_app.clone();
        move || {
            let version = format!(" v{}", env!("CARGO_PKG_VERSION"));
            let about_window = AboutWindow::new().unwrap();
            about_window.set_app_name(slint::SharedString::from(APP_NAME));
            about_window.set_app_version(slint::SharedString::from(version));
            about_window.set_hidden(speech_app.lock().config.hidden);

            about_window.on_hidden_cb_toggled({
                let weak_about_window = about_window.as_weak();
                let speech_app = speech_app.clone();
                move || {
                    let about_window = weak_about_window.unwrap();
                    speech_app.lock().config.hidden = about_window.get_hidden();
                    speech_app.lock().config.store();
                }
            });

            about_window.show().unwrap();
        }
    });
    app_window.on_test_button_clicked({
        let weak_app_window = app_window.as_weak();
        let speech_app = speech_app.clone();
        move || {
            let app_window = weak_app_window.unwrap();
            speech_app
                .lock()
                .speak(&app_window.get_test_string())
                .unwrap();
        }
    });
    app_window.on_dict_button_clicked({
        let speech_app = speech_app.clone();
        move || {
            let dictionary_window = DictionaryWindow::new().unwrap();

            dictionary_window.set_word_list_model(speech_app.lock().dictionary.get_model());

            dictionary_window.on_add_button_clicked({
                let weak_dictionary_window = dictionary_window.as_weak();
                let speech_app = speech_app.clone();
                move || {
                    let dictionary_window = weak_dictionary_window.unwrap();

                    let old_word: String = String::from(
                        dictionary_window
                            .get_old_word_string()
                            .trim()
                            .to_lowercase(),
                    )
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
                    let new_word: String = String::from(
                        dictionary_window
                            .get_new_word_string()
                            .trim()
                            .to_lowercase(),
                    )
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
                    if !old_word.is_empty() && !new_word.is_empty() {
                        speech_app
                            .lock()
                            .dictionary
                            .word_map
                            .insert(String::from(old_word), String::from(new_word));

                        if let Some(path) = get_dictionary_path() {
                            let _ = speech_app.lock().dictionary.save(&path);
                        }
                        dictionary_window
                            .set_word_list_model(speech_app.lock().dictionary.get_model());
                    }
                }
            });
            dictionary_window.on_delete_button_clicked({
                let weak_dictionary_window = dictionary_window.as_weak();
                let speech_app = speech_app.clone();
                move || {
                    let dictionary_window = weak_dictionary_window.unwrap();

                    let item = String::from(dictionary_window.invoke_word_list_get_current_item());
                    if !item.is_empty() {
                        if let Some(kv) = item.split_once('=') {
                            speech_app.lock().dictionary.word_map.remove(kv.0);

                            if let Some(path) = get_dictionary_path() {
                                let _ = speech_app.lock().dictionary.save(&path);
                            }
                            dictionary_window
                                .set_word_list_model(speech_app.lock().dictionary.get_model());
                        }
                    }
                }
            });

            dictionary_window.show().unwrap();
        }
    });
    app_window.on_output_cb_selected({
        let weak_app_window = app_window.as_weak();
        let speech_app = speech_app.clone();
        move |_value: slint::SharedString| {
            let app_window = weak_app_window.unwrap();
            let index = app_window.get_output_cb_current_index();
            match index {
                0 => speech_app.lock().set_output(OutputKind::Speak),
                1 => speech_app.lock().set_output(OutputKind::SaveWAV),
                2 => speech_app.lock().set_output(OutputKind::SaveOpus),
                3 => speech_app.lock().set_output(OutputKind::SaveOggVorbis),
                _ => (),
            }
        }
    });
    app_window.on_output_button_clicked({
        move || {
            if let Some(path) = get_recordings_path() {
                std::process::Command::new("explorer")
                    .arg(path)
                    .spawn()
                    .unwrap();
            }
        }
    });
    app_window.on_link_ta_clicked({
        move || {
            open::that("https://simplettsreader.sourceforge.io/").unwrap();
        }
    });

    if !speech_app.lock().config.hidden {
        app_window.show()?;
    }
    slint::run_event_loop_until_quit()?;
    app_window.hide()?;

    Ok(())
}
