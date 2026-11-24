//! Drop-in `dialoguer` wrappers: same builder API for the parts this crate uses, but every
//! `interact*` runs through [`super::with_prompt`], so a prompt reached while the full-screen
//! shell is up first gives the terminal back (see `shell::prompt`). With the shell off these are
//! exactly the `dialoguer` types.
//!
//! Callers only change their import: `use sap_automation::shell::dialog::{Input, Select};`.
#![allow(dead_code)]

use super::with_prompt;
use std::fmt::Debug;
use std::str::FromStr;

type Result<T> = dialoguer::Result<T>;

pub struct Select<'a> {
    inner: dialoguer::Select<'a>,
    prompt: String,
    items: Vec<String>,
    default: usize,
}

impl Default for Select<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl Select<'static> {
    pub fn new() -> Self {
        Select {
            inner: dialoguer::Select::new(),
            prompt: String::new(),
            items: Vec::new(),
            default: 0,
        }
    }
}

impl Select<'_> {
    pub fn default(mut self, val: usize) -> Self {
        self.default = val;
        self.inner = self.inner.default(val);
        self
    }
    pub fn clear(mut self, val: bool) -> Self {
        self.inner = self.inner.clear(val);
        self
    }
    pub fn max_length(mut self, val: usize) -> Self {
        self.inner = self.inner.max_length(val);
        self
    }
    pub fn item<T: ToString>(mut self, item: T) -> Self {
        self.items.push(item.to_string());
        self.inner = self.inner.item(item);
        self
    }
    pub fn items<T, I>(mut self, items: I) -> Self
    where
        T: ToString,
        I: IntoIterator<Item = T>,
    {
        let items: Vec<T> = items.into_iter().collect();
        self.items.extend(items.iter().map(|i| i.to_string()));
        self.inner = self.inner.items(items);
        self
    }
    pub fn with_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        let prompt = prompt.into();
        self.prompt = prompt.clone();
        self.inner = self.inner.with_prompt(prompt);
        self
    }
    pub fn report(mut self, val: bool) -> Self {
        self.inner = self.inner.report(val);
        self
    }

    /// Title and default for the in-shell menu, `None` when it cannot be shown there.
    fn shell_menu(&self) -> Option<(String, Option<usize>)> {
        if self.items.is_empty() {
            return None;
        }
        let title = if self.prompt.is_empty() {
            "Choose an option".to_string()
        } else {
            self.prompt.clone()
        };
        Some((title, Some(self.default.min(self.items.len() - 1))))
    }

    /// Inside an interactive shell session the choice is a shell menu (header and footer stay
    /// visible); `None` = not available, use the terminal prompt. `Some(Ok(None))` = Esc / q.
    fn ask_in_shell(&self) -> Option<std::io::Result<Option<usize>>> {
        let (title, default) = self.shell_menu()?;
        super::try_menu(&title, &self.items, default)
    }

    pub fn interact(self) -> Result<usize> {
        loop {
            match self.ask_in_shell() {
                Some(Ok(Some(i))) => return Ok(i),
                // `interact` has no cancel: Esc asks again while the shell is still up
                Some(Ok(None)) if super::request::requests_available() => continue,
                Some(Err(e)) => return Err(e.into()),
                _ => break,
            }
        }
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.inner.interact())
    }
    pub fn interact_opt(self) -> Result<Option<usize>> {
        match self.ask_in_shell() {
            Some(Ok(choice)) => return Ok(choice),
            Some(Err(e)) => return Err(e.into()),
            None => {}
        }
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.inner.interact_opt())
    }
}

pub struct Confirm<'a>(dialoguer::Confirm<'a>);

impl Default for Confirm<'static> {
    fn default() -> Self {
        Confirm(dialoguer::Confirm::new())
    }
}

impl Confirm<'static> {
    pub fn new() -> Self {
        Confirm(dialoguer::Confirm::new())
    }
}

impl Confirm<'_> {
    pub fn with_prompt<S: Into<String>>(self, prompt: S) -> Self {
        Confirm(self.0.with_prompt(prompt))
    }
    pub fn report(self, val: bool) -> Self {
        Confirm(self.0.report(val))
    }
    pub fn wait_for_newline(self, wait: bool) -> Self {
        Confirm(self.0.wait_for_newline(wait))
    }
    pub fn default(self, val: bool) -> Self {
        Confirm(self.0.default(val))
    }
    pub fn show_default(self, val: bool) -> Self {
        Confirm(self.0.show_default(val))
    }
    pub fn interact(self) -> Result<bool> {
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.0.interact())
    }
    pub fn interact_opt(self) -> Result<Option<bool>> {
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.0.interact_opt())
    }
}

pub struct Input<'a, T>(dialoguer::Input<'a, T>);

impl<T> Default for Input<'static, T> {
    fn default() -> Self {
        Input(dialoguer::Input::new())
    }
}

impl<T> Input<'_, T> {
    pub fn new() -> Self {
        Input(dialoguer::Input::new())
    }
    pub fn with_prompt<S: Into<String>>(self, prompt: S) -> Self {
        Input(self.0.with_prompt(prompt))
    }
    pub fn report(self, val: bool) -> Self {
        Input(self.0.report(val))
    }
    pub fn with_initial_text<S: Into<String>>(self, val: S) -> Self {
        Input(self.0.with_initial_text(val))
    }
    pub fn default(self, value: T) -> Self {
        Input(self.0.default(value))
    }
    pub fn allow_empty(self, val: bool) -> Self {
        Input(self.0.allow_empty(val))
    }
    pub fn show_default(self, val: bool) -> Self {
        Input(self.0.show_default(val))
    }
}

impl<'a, T: 'a> Input<'a, T>
where
    T: Clone + ToString + FromStr,
    <T as FromStr>::Err: Debug + ToString,
{
    pub fn validate_with<V, E>(self, validator: V) -> Self
    where
        V: FnMut(&T) -> std::result::Result<(), E> + 'a,
        E: ToString,
    {
        Input(self.0.validate_with(validator))
    }
}

impl<T> Input<'_, T>
where
    T: Clone + ToString + FromStr,
    <T as FromStr>::Err: ToString,
{
    pub fn interact_text(self) -> Result<T> {
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.0.interact_text())
    }
}

impl<T> Input<'_, T>
where
    T: Clone + ToString + FromStr,
    <T as FromStr>::Err: Debug + ToString,
{
    pub fn interact(self) -> Result<T> {
        super::prompt::ensure_can_prompt()?;
        with_prompt(|| self.0.interact())
    }
}
