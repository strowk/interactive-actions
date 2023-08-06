//!
//! doc for module
//!
use anyhow::Result;
use requestty::{Answer, Question};
use std::collections::BTreeMap;

use requestty_ui::backend::{Size, TestBackend};
use requestty_ui::events::{KeyEvent, TestEvents};
use serde_derive::{Deserialize, Serialize};
use std::vec::IntoIter;

fn default<T: Default + PartialEq>(t: &T) -> bool {
    *t == Default::default()
}

#[doc(hidden)]
pub type VarBag = BTreeMap<String, String>;

///
/// When in the workflow to hook the action
///
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActionHook {
    /// Run after actions
    #[default]
    #[serde(rename = "after")]
    After,

    /// Run before actions
    #[serde(rename = "before")]
    Before,
}
///
/// [`Action`] defines the action to run:
/// * script
/// * interaction
/// * control flow and variable capture
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    /// unique name of action
    pub name: String,

    /// interaction
    #[serde(default)]
    pub interaction: Option<Interaction>,

    /// a run script
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,

    /// ignore exit code from the script, otherwise if error then exists
    ///
    #[serde(default)]
    #[serde(skip_serializing_if = "default")]
    pub ignore_exit: bool,

    /// if confirm cancel, cancel all the rest of the actions and break out
    #[serde(default)]
    #[serde(skip_serializing_if = "default")]
    pub break_if_cancel: bool,

    /// captures the output of the script, otherwise, stream to screen in real time
    #[serde(default)]
    #[serde(skip_serializing_if = "default")]
    pub capture: bool,

    /// When to run this action
    #[serde(default)]
    #[serde(skip_serializing_if = "default")]
    pub hook: ActionHook,
}
///
/// result of the [`Action`]
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionResult {
    /// name of action that was run
    pub name: String,
    /// result of run script
    pub run: Option<RunResult>,
    /// interaction response, if any
    pub response: Response,
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub script: String,
    pub code: i32,
    pub out: String,
    pub err: String,
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteractionKind {
    #[serde(rename = "confirm")]
    Confirm,
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "select")]
    Select,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Text(String),
    Cancel,
    None,
}

///
/// [`Interaction`] models an interactive session with a user declaratively
/// You can pick from _confirm_, _input_, and other modes of prompting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Interaction {
    /// type of interaction
    pub kind: InteractionKind,
    /// what to ask the user
    pub prompt: String,

    /// if set, capture the value of answer, and set it to a variable name defined here
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,

    /// define the set of options just for kind=select
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,

    /// default value of interaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<DefaultValue>,

    /// perform this interaction even if default is supplied, default is to skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_if_has_default: Option<bool>,
}

/// default value of interaction, depending on the type of interaction
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DefaultValue {
    /// default value for text input
    Input(String),
    /// default value for select - index of the option
    Select(usize),
    /// default value for confirm - true or false
    Confirm(bool),
}

impl Interaction {
    fn update_varbag(&self, input: &str, varbag: Option<&mut VarBag>) {
        varbag.map(|bag| {
            self.out
                .as_ref()
                .map(|out| bag.insert(out.to_string(), input.to_string()))
        });
    }

    /// Play an interaction
    ///
    /// # Errors
    ///
    /// This function will return an error if text input failed
    pub fn play(
        &self,
        varbag: Option<&mut VarBag>,
        events: Option<&mut TestEvents<IntoIter<KeyEvent>>>,
    ) -> Result<Response> {
        let question = self.to_question();
        let mut prompt = requestty::PromptModule::new([question]);
        let answer = self.to_default_answer();
        if let Some(answer) = answer {
            prompt = prompt.with_answers(requestty::Answers::from_iter(
                [("question".to_string(), answer)].into_iter(),
            ));
        }

        if let Some(events) = events {
            let mut backend = TestBackend::new(Size::from((50, 20)));
            prompt.prompt_with(&mut backend, events)
        } else {
            prompt.prompt()
        }?;

        let answers = prompt.into_answers();
        let answer = answers.get("question");

        Ok(match answer {
            Some(Answer::String(input)) => {
                self.update_varbag(&input, varbag);

                Response::Text(input.to_string())
            }
            Some(Answer::ListItem(selected)) => {
                self.update_varbag(&selected.text, varbag);
                Response::Text(selected.text.clone())
            }
            Some(Answer::Bool(confirmed)) if *confirmed => {
                let as_string = "true".to_string();
                self.update_varbag(&as_string, varbag);
                Response::Text(as_string)
            }
            None => {
                Response::Cancel
                // this is not supposed to happen
            }
            _ => {
                Response::Cancel
                // not supported question types
            }
        })
    }

    fn to_default_answer(&self) -> Option<Answer> {
        if let Some(default) = &self.default_value {
            Some(match default {
                DefaultValue::Input(ref input) => Answer::String(input.clone()),
                DefaultValue::Select(index) => Answer::ListItem(requestty::ListItem {
                    text: self.options.as_ref().unwrap()[*index].clone(),
                    index: *index,
                }),
                DefaultValue::Confirm(confirmed) => Answer::Bool(*confirmed),
            })
        } else {
            None
        }
    }

    /// Convert the interaction into a question
    pub fn to_question(&self) -> Question<'_> {
        match self.kind {
            InteractionKind::Input => {
                let builder = Question::input("question").message(self.prompt.clone());
                if let Some(ask) = self.ask_if_has_default {
                    if ask {
                        builder.ask_if_answered(ask)
                    } else {
                        builder
                    }
                } else {
                    builder
                }
                .build()
            }
            InteractionKind::Select => {
                let builder = Question::select("question")
                    .message(self.prompt.clone())
                    .choices(self.options.clone().unwrap_or_default());
                if let Some(ask) = self.ask_if_has_default {
                    if ask {
                        builder.ask_if_answered(ask)
                    } else {
                        builder
                    }
                } else {
                    builder
                }
                .build()
            }
            InteractionKind::Confirm => {
                let builder = Question::confirm("question").message(self.prompt.clone());
                if let Some(ask) = self.ask_if_has_default {
                    if ask {
                        builder.ask_if_answered(ask)
                    } else {
                        builder
                    }
                } else {
                    builder
                }
                .build()
            }
        }
    }
}
