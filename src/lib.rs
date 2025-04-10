slint::include_modules!();

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::{Arc, Weak};

const APP_NAME: &str = "Simple TTS Reader";

#[derive(Serialize, Deserialize, PartialEq)]
struct Config {
    voice_name: String,
    rate: i32,
    volume: u32,
    hidden: bool,
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
        _ = confy::store(APP_NAME, "config", self);
    }
}

impl std::default::Default for Config {
    fn default() -> Self {
        Self {
            voice_name: String::from(""),
            rate: 0,
            volume: 100,
            hidden: false,
        }
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

struct SapiEventHandler;

impl sapi_lite::tts::EventHandler for SapiEventHandler {
    fn on_speech_finished(&self, _id: u32) {}
}

struct SpeechApp {
    synth: sapi_lite::tts::EventfulSynthesizer,
    voices: Vec<sapi_lite::tts::Voice>,
    config: Config,
}

impl SpeechApp {
    fn build(config: Config) -> Result<Self, Box<dyn Error>> {
        sapi_lite::initialize()?;

        let mut speech_app = Self {
            synth: sapi_lite::tts::EventfulSynthesizer::new(SapiEventHandler)?,
            voices: sapi_lite::tts::installed_voices(None, None)?.collect(),
            config,
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
            String::from("")
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

    fn speak(&mut self, speech: &str) -> Result<u32, Box<dyn Error>> {
        // TODO: Find a better way to stop active speech
        self.synth = sapi_lite::tts::EventfulSynthesizer::new(SapiEventHandler)?;

        self.set_voice(None)?;
        self.set_rate(None)?;
        self.set_volume(None)?;

        Ok(self.synth.speak(speech)?)
    }
}

impl Drop for SpeechApp {
    fn drop(&mut self) {
        sapi_lite::finalize();
    }
}

struct ClipboardListener {
    clipboard: arboard::Clipboard,
    speech_app: Weak<Mutex<SpeechApp>>,
}

impl ClipboardListener {
    fn spawn(speech_app: Weak<Mutex<SpeechApp>>) {
        std::thread::spawn(move || {
            let Ok(clipboard) = arboard::Clipboard::new() else {
                return;
            };

            let listener = ClipboardListener {
                clipboard,
                speech_app,
            };

            let _ = clipboard_master::Master::new(listener).run();
        });
    }
}

impl clipboard_master::ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> clipboard_master::CallbackResult {
        if let Ok(text) = self.clipboard.get_text() {
            if let Some(speech_app) = Weak::upgrade(&self.speech_app) {
                let _ = speech_app.lock().speak(&text);
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

    let speech_app = Arc::new(Mutex::new(SpeechApp::build(config)?));

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
            if let tray_icon::TrayIconEvent::DoubleClick { .. } = event {
                let weak_app_window = weak_app_window.clone();
                slint::invoke_from_event_loop(move || {
                    let app_window = weak_app_window.unwrap();
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

    if !speech_app.lock().config.hidden {
        app_window.show()?;
    }
    slint::run_event_loop_until_quit()?;
    app_window.hide()?;

    Ok(())
}
