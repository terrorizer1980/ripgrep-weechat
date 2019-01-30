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
        let tab_count = line.matches("\t").count();

        let (date, nick, msg) = if tab_count >= 2 {
            let vec: Vec<&str> = line.splitn(3, "\t").collect();
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
        format!(
            "{date_color}{date} {nick_color}{nick}{reset_color} {msg}",
            date_color=weechat.color("blue"),
            date=date,
            nick_color=weechat.color("green"),
            nick=nick,
            reset_color=weechat.color("reset"),
            msg=msg
        )
    }

    pub fn print(&self, line: &str) {
        self.buffer.print(&self.format_line(line));
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
