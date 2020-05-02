use weechat::Weechat;
use clap::{App, Arg};
use std::rc::Rc;
use std::cell::RefCell;
use weechat::buffer::{Buffer, BufferHandle, BufferSettings, BufferInputCallbackAsync, BufferSettingsAsync};
use std::borrow::Cow;
use crate::{Ripgrep, CommandData};
use grep_regex::RegexMatcher;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use grep_searcher::Searcher;
use tokio::runtime::Runtime;
use async_trait::async_trait;

pub(crate) struct GrepBuffer {
    buffer: BufferHandle,
    grep: Grep,
}

#[derive(Clone)]
pub struct Grep {
    command_data: CommandData,
}

#[async_trait(?Send)]
impl BufferInputCallbackAsync for Grep {
    async fn callback(&mut self, buffer: BufferHandle, input: String) {
        let buffer = buffer.upgrade().unwrap();

        let file = Ripgrep::get_file_by_buffer(&buffer);

        let file = match file {
            Some(f) => f,
            None => return,
        };

        let matcher = match RegexMatcher::new(&input) {
            Ok(m) => m,
            Err(_) => {
                Weechat::print("Invalid regex");
                return;
            }
        };

        let (tx, rx) = channel(1);

        self.command_data.runtime.borrow_mut().as_ref().unwrap().spawn(Ripgrep::search(file, matcher, tx));
        Ripgrep::recieve_result(self.command_data.clone(), rx).await;
    }
}

impl GrepBuffer {
    pub(crate) fn new(command_data: &CommandData) -> GrepBuffer {
        let grep = Grep { command_data: command_data.clone() };
        let settings = BufferSettingsAsync::new("ripgrep").input_callback(grep.clone());
        let buffer_handle = Weechat::buffer_new_with_async(settings).unwrap();
        let buffer = buffer_handle.upgrade().unwrap();

        buffer.disable_nicklist();
        buffer.disable_time_for_each_line();
        buffer.disable_log();
        buffer.set_title("ripgrep output buffer");

        GrepBuffer { buffer: buffer_handle, grep }
    }

    fn split_line(line: &str) -> (&str, &str, String) {
        let tab_count = line.matches('\t').count();

        let (date, nick, msg) = if tab_count >= 2 {
            let vec: Vec<&str> = line.splitn(3, '\t').collect();
            (vec[0], vec[1], vec[2])
        } else {
            ("", "", line)
        };

        let msg = msg.trim().replace("\t", "    ");
        (date.trim(), nick.trim(), msg)
    }

    pub fn format_line(&self, line: &str) -> String {
        let (date, nick, msg) = GrepBuffer::split_line(line);
        let nick = self.colorize_nick(nick);
        format!(
            "{date_color}{date}{reset} {nick} {msg}",
            date_color=Weechat::color("brown"),
            date=date,
            reset=Weechat::color("reset"),
            nick=nick,
            msg=msg
        )
    }

    pub fn print(&self, line: &str) {
        self.buffer.upgrade().unwrap().print(&self.format_line(line));
    }

    pub fn colorize_nick(&self, nick: &str) -> String {
        if nick.is_empty() {
            return "".to_owned();
        }

        // TODO colorize the nick prefix and suffix
        // TODO handle the extra nick prefix and suffix settings

        let (prefix, nick) = {
            let first_char = nick.chars().next();
            match first_char {
                Some('&') | Some('@') | Some('!') | Some('+') | Some('%') => {
                    (first_char, &nick[1..])
                }
                Some(_)   => (None, nick),
                None      => (None, nick),
            }
        };

        let prefix = match prefix {
            Some(p) => p.to_string(),
            None => "".to_owned()
        };

        // let nick_color = Weechat::info_get("nick_color_name", nick).unwrap();

        format!(
            "{}{}{}{}",
            prefix,
            Weechat::color("blue"),
            nick,
            Weechat::color("reset")
        )
    }

    pub fn print_status(&self, line: &str) {
        self.buffer.upgrade().unwrap().print(
            &format!(
                "{}[{}grep{}]{}\t{}",
                Weechat::color("chat_delimiters"),
                Weechat::color("chat_nick"),
                Weechat::color("chat_delimiters"),
                Weechat::color("reset"),
                line
            )
        )
    }

    pub fn switch_to(&self) {
        self.buffer.upgrade().unwrap().switch_to();
    }
}
