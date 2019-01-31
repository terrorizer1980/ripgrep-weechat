#[macro_use]
extern crate weechat;

extern crate pipe_channel;

extern crate clap;

extern crate grep_matcher;
extern crate grep_regex;
extern crate grep_searcher;

mod buffer;

use clap::{App, Arg};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use std::io;
use grep_searcher::sinks::Lossy;

use std::{thread};
use pipe_channel::{channel, Sender, Receiver};

use weechat::{
    Weechat,
    WeechatPlugin,
    ArgsWeechat,
    WeechatResult,
    FdHook,
    CommandHook,
    CommandDescription,
    Buffer
};
use weechat::hooks::FdHookMode;

use buffer::GrepBuffer;

static mut _WEECHAT: Option<Weechat> = None;

type SearchResult = Result<Vec<String>, io::Error>;


struct Ripgrep {
    thread: Option<thread::JoinHandle<SearchResult>>,
    fd_hook: Option<FdHook<(), Receiver<ThreadMsg>>>,
    _command: CommandHook<()>
}

enum ThreadMsg {
    Done,
}

fn get_weechat() -> &'static mut Weechat {
    unsafe {
        match &mut _WEECHAT {
            Some(x) => x,
            None => panic!(),
        }
    }
}

fn get_plugin() -> &'static mut Ripgrep {
    unsafe {
        match &mut __PLUGIN {
            Some(x) => x,
            None => panic!(),
        }
    }
}

impl Ripgrep {
    fn join_thread(&mut self) {
        let weechat = get_weechat();

        let handle = self.thread.take();

        let handle = match handle {
            Some(h) => h,
            None => return
        };
        let result = handle.join();

        match result {
            Ok(result) => Ripgrep::print_results(result),
            Err(_) => weechat.print("Error in search thread")
        };
    }

    fn print_results(result: SearchResult) {
        let weechat = get_weechat();

        let rgbuffer = GrepBuffer::get_buffer(weechat);

        rgbuffer.print_status("Search for TODO");
        match result {
            Ok(result) => {
                for line in result {
                    rgbuffer.print(&line);
                }
            }
            Err(errno) => {
                weechat.print(
                    &format!("Error searching: {}", errno.to_string())
                )
            }
        }
        rgbuffer.print_status("Summary of search TODO");
    }

    fn fd_hook_cb(_data: &(), receiver: &mut Receiver<ThreadMsg>) {
        let plugin = get_plugin();

        match receiver.recv() {
            Ok(_) => {
                plugin.join_thread();
            }
            Err(_) => {
                plugin.join_thread();
            }
        }

        plugin.fd_hook = None;
    }

    fn file_from_infolist(buffer: &Buffer) -> String {
        let weechat = get_weechat();
        let infolist = weechat.infolist_get("logger_buffer", "");

        let infolist = match infolist {
            Some(list) => list,
            None => return "".to_owned()
        };

        while infolist.next() {
            let other_buffer = infolist.get_buffer();
            match other_buffer {
                Some(other_buffer) => {
                    if *buffer == other_buffer {
                        let path = infolist.get_string("log_filename");
                        return path.to_owned()
                    }
                }

                None => continue
            }
        }

        "".to_owned()
    }

    fn file_from_name(full_name: &str) -> PathBuf {
        let weechat = get_weechat();
        let weechat_home = weechat.info_get("weechat_dir", "");
        let mut file = Path::new(weechat_home).join("logs");
        let mut full_name = full_name.to_owned();
        full_name.push_str(".weechatlog");
        file.push(full_name);
        file
    }

    fn get_file_by_buffer(buffer: Buffer) -> Option<PathBuf> {
        let path = Ripgrep::file_from_infolist(&buffer);

        if path.is_empty() {
            let full_name = buffer.full_name().to_string().to_lowercase();
            return Some(Ripgrep::file_from_name(&full_name));
        }

        let path = PathBuf::from_str(&path);

        match path {
            Ok(p) => Some(p),
            Err(_) => None
        }
    }

    fn search(
        file: PathBuf,
        matcher: RegexMatcher,
        mut sender: Sender<ThreadMsg>
    ) -> SearchResult {
        let mut matches: Vec<String> = vec![];

        let sink = Lossy(|_, line| {
            matches.push(line.to_string());
            Ok(true)
        });

        Searcher::new().search_path(&matcher, file, sink)?;

        sender.send(ThreadMsg::Done).unwrap();
        Ok(matches)
    }

    fn command_cb(_data: &(), buffer: Buffer, args: ArgsWeechat) {
        let plugin = get_plugin();
        let weechat = get_weechat();
        let parsed_args = App::new("rg")
            .arg(Arg::with_name("pattern")
                               .index(1)
                               .value_name("PATTERN")
                               .help("A regular expression used for
                                     searching.")
                               .multiple(true))
            .get_matches_from_safe(args).unwrap();

        let file = Ripgrep::get_file_by_buffer(buffer);

        let file = match file {
            Some(f) => f,
            None => return
        };

        let pattern = match parsed_args.value_of("pattern") {
            Some(p) => p,
            None => {
                weechat.print("Invalid pattern");
                return
            }
        };

        let matcher = match RegexMatcher::new(pattern) {
            Ok(m) => m,
            Err(_) => {
                weechat.print("Invalid regex");
                return
            }
        };

        let (tx, rx) = channel();

        let fd_hook = weechat.hook_fd(
            rx,
            FdHookMode::Write,
            Ripgrep::fd_hook_cb,
            None,
        );

        let handle = thread::spawn(
            move || Ripgrep::search(file, matcher, tx)
        );

        plugin.thread = Some(handle);
        plugin.fd_hook = Some(fd_hook);
    }
}

impl WeechatPlugin for Ripgrep {
    fn init(weechat: Weechat, _args: ArgsWeechat) -> WeechatResult<Self> {
        let command_info = CommandDescription {
            name: "rg",
            ..Default::default()
        };

        let command = weechat.hook_command(
            command_info,
            Ripgrep::command_cb,
            None
        );

        unsafe {
            _WEECHAT = Some(weechat);
        }

        Ok(Ripgrep {
            thread: None,
            fd_hook: None,
            _command: command
        })
    }
}

impl Drop for Ripgrep {
    fn drop(&mut self) {
    }
}

weechat_plugin!(
    Ripgrep,
    name: b"ripgrep\0"; 8,
    author: b"Damir Jelic <poljar@termina.org.uk>\0"; 36,
    description: b"Search in buffers and logs using ripgrep\0"; 41,
    version: b"0.1.0\0"; 6,
    license: b"ISC\0"; 4
);
