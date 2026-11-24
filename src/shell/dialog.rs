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

pub struct Select<'a>(dialoguer::Select<'a>);

impl Default for Select<'static> {
    fn default() -> Self {
        Select(dialoguer::Select::new())
    }
}

impl Select<'static> {
    pub fn new() -> Self {
        Select(dialoguer::Select::new())
    }
}

impl Select<'_> {
    pub fn default(self, val: usize) -> Self {
        Select(self.0.default(val))
    }
    pub fn clear(self, val: bool) -> Self {
        Select(self.0.clear(val))
    }
    pub fn max_length(self, val: usize) -> Self {
        Select(self.0.max_length(val))
    }
    pub fn item<T: ToString>(self, item: T) -> Self {
        Select(self.0.item(item))
    }
    pub fn items<T, I>(self, items: I) -> Self
    where
        T: ToString,
        I: IntoIterator<Item = T>,
    {
        Select(self.0.items(items))
    }
    pub fn with_prompt<S: Into<String>>(self, prompt: S) -> Self {
        Select(self.0.with_prompt(prompt))
    }
    pub fn report(self, val: bool) -> Self {
        Select(self.0.report(val))
    }
    pub fn interact(self) -> Result<usize> {
        with_prompt(|| self.0.interact())
    }
    pub fn interact_opt(self) -> Result<Option<usize>> {
        with_prompt(|| self.0.interact_opt())
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
        with_prompt(|| self.0.interact())
    }
    pub fn interact_opt(self) -> Result<Option<bool>> {
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
        with_prompt(|| self.0.interact_text())
    }
}

impl<T> Input<'_, T>
where
    T: Clone + ToString + FromStr,
    <T as FromStr>::Err: Debug + ToString,
{
    pub fn interact(self) -> Result<T> {
        with_prompt(|| self.0.interact())
    }
}
