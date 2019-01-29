#[macro_use]
extern crate weechat;

extern crate pipe_channel;

extern crate clap;

extern crate grep_matcher;
extern crate grep_regex;
extern crate grep_searcher;

use clap::{App, Arg};

use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::Lossy;

use std::{thread, time};
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

static mut _weechat: Option<Weechat> = None;

type SearchResult = Result<Vec<String>, i32>;


struct Ripgrep {
    thread: Option<thread::JoinHandle<()>>,
    fd_hook: Option<FdHook<(), Receiver<SearchResult>>>,
    command: CommandHook<()>
}

fn get_weechat() -> &'static mut Weechat {
    unsafe {
        match &mut _weechat {
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
        self.thread = None;
    }

    fn fd_hook_cb(_data: &(), receiver: &mut Receiver<SearchResult>) {
        let weechat = get_weechat();
        let plugin = get_plugin();

        match receiver.recv() {
            Ok(data) => {
                match data {
                    Ok(lines) => {
                        for line in lines {
                            weechat.print(&line);
                        }
                    }
                    Err(errno) => {
                        weechat.print(
                            &format!("Error searching: {}", errno.to_string())
                        )
                    }
                }
            }
            Err(_) => {
                plugin.join_thread();
                plugin.fd_hook = None;
            }
        }
    }

    fn search(
        file: String,
        matcher: RegexMatcher,
        mut sender: Sender<SearchResult>
    ) {
        let mut matches: Vec<String> = vec![];

        let sink = Lossy(|_, line| {
            matches.push(line.to_string());
            Ok(true)
        });

        Searcher::new().search_path(&matcher, file, sink);

        sender.send(Ok(matches)).unwrap();
    }

    fn command_cb(_data: &(), buffer: Buffer, args: ArgsWeechat) {
        let plugin = get_plugin();
        let weechat = get_weechat();
        buffer.print("Sending to thread!");

        let parsed_args = App::new("rg")
            .arg(Arg::with_name("file")
                               .short("f")
                               .long("file")
                               .value_name("FILE")
                               .help("File to search")
                               .takes_value(true))
            .get_matches_from_safe(args).unwrap();

        if let Some(file) = parsed_args.value_of("file") {
            let file = file.to_string();
            let matcher = match RegexMatcher::new(r"weechat\.") {
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
            _weechat = Some(weechat);
        }

        Ok(Ripgrep {
            thread: None,
            fd_hook: None,
            command
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
