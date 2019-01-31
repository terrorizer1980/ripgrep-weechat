extern crate weechat;

use weechat::{
    Weechat,
    Buffer
};

pub(crate) struct GrepBuffer {
    buffer: Buffer
}

impl GrepBuffer {
    pub(crate) fn new(weechat: &Weechat) -> GrepBuffer {
        let buffer = weechat.buffer_new(
            "ripgrep",
            None,
            None::<()>,
            None,
            None::<()>,
        );
        buffer.disable_nicklist();
        buffer.disable_time_for_each_line();
        buffer.disable_log();
        buffer.set_title("ripgrep output buffer");
        GrepBuffer { buffer }
    }

    pub(crate) fn from_buffer(buffer: Buffer) -> GrepBuffer {
        GrepBuffer { buffer }
    }

    pub(crate) fn get_buffer(weechat: &Weechat) -> GrepBuffer {
        let rgbuffer = weechat.buffer_search("ripgrep", "ripgrep");

        match rgbuffer {
            Some(buffer) => GrepBuffer::from_buffer(buffer),
            None => GrepBuffer::new(weechat)

        }
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
        let weechat = self.buffer.get_weechat();
        let (date, nick, msg) = GrepBuffer::split_line(line);
        let nick = self.colorize_nick(nick);
        format!(
            "{date_color}{date}{reset} {nick} {msg}",
            date_color=weechat.color("brown"),
            date=date,
            reset=weechat.color("reset"),
            nick=nick,
            msg=msg
        )
    }

    pub fn print(&self, line: &str) {
        self.buffer.print(&self.format_line(line));
    }

    pub fn colorize_nick(&self, nick: &str) -> String {
        if nick.is_empty() {
            return "".to_owned();
        }

        let weechat = self.buffer.get_weechat();
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

        let nick_color = weechat.info_get("nick_color_name", nick);

        format!(
            "{}{}{}{}",
            prefix,
            weechat.color(nick_color),
            nick,
            weechat.color("reset")
        )
    }

    pub fn print_status(&self, line: &str) {
        let weechat = self.buffer.get_weechat();
        self.buffer.print(
            &format!(
                "{}[{}grep{}]{}\t{}",
                weechat.color("chat_delimiters"),
                weechat.color("chat_nick"),
                weechat.color("chat_delimiters"),
                weechat.color("reset"),
                line
            )
        )
    }
}
