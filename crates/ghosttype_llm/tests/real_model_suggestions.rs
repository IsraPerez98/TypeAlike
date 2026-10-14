//! Suggests text with a real GGUF model while the user types. Run it with
//! `GHOSTTYPE_TEST_MODEL=/path/to/model.gguf cargo test -p ghosttype_llm -- --ignored`.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use ghosttype_core::app::{App, Field};
use ghosttype_core::engine::Reaction;
use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner,
};
use ghosttype_core::key::Key;
use ghosttype_llm::LlamaCpp;

struct LoadedModel {
    runner: LlamaCpp,
    sink: InferenceEventSink,
    events: mpsc::Receiver<InferenceEvent>,
}

impl LoadedModel {
    fn load() -> Self {
        let path =
            PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
        let mut runner = LlamaCpp::new(move |_| Some(path.clone()));
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        let file = ModelFile {
            repo: "test/model".to_owned(),
            name: "model.gguf".to_owned(),
        };
        runner.submit(InferenceRequest::Load { file: file.clone() }, &sink);
        let loaded = events.recv().expect("the runner answers the load");
        assert_eq!(loaded, InferenceEvent::Loaded { file, result: Ok(()) });
        Self { runner, sink, events }
    }

    fn complete(&mut self, reaction: Reaction) -> Result<Completion, InferenceError> {
        let Reaction::Request(request) = reaction else {
            panic!("expected a request, got {reaction:?}");
        };
        self.runner.submit(request, &self.sink);
        let event = self.events.recv().expect("the runner answers the completion");
        let InferenceEvent::Completed(result) = event else {
            panic!("expected a completion, got {event:?}");
        };
        result
    }
}

#[test]
#[ignore = "needs the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
fn real_model_suggests_text_after_typed_words() {
    let mut model = LoadedModel::load();
    let mut app = App::new();
    let first_request = app.on_key(Key::Char('T'), Field::Open);
    "hank you for your"
        .chars()
        .for_each(|character| assert_eq!(app.on_key(Key::Char(character), Field::Open), Reaction::Hide));

    let stale = app.on_completion(model.complete(first_request));
    let reaction = app.on_completion(model.complete(stale));

    assert!(matches!(reaction, Reaction::Show(_)), "unexpected {reaction:?}");
}
