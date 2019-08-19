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
    Buffer,
    Config,
    ConfigSectionInfo,
    BooleanOption
};
use weechat::hooks::FdHookMode;
use weechat::weechat_plugin;
use weechat::config_options::ConfigOption;

use buffer::GrepBuffer;

static mut _WEECHAT: Option<Weechat> = None;

type SearchResult = Result<Vec<String>, io::Error>;


struct Ripgrep {
    thread: Option<thread::JoinHandle<SearchResult>>,
    fd_hook: Option<FdHook<(), Receiver<ThreadMsg>>>,
    _config: Config<()>,
    go_to_buffer_opt: BooleanOption,
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
        let plugin = get_plugin();

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

        if plugin.go_to_buffer_opt.value() {
            rgbuffer.switch_to();
        }

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
                        match path {
                            Some(p) => return p.to_string(),
                            None => continue
                        }
                    }
                }

                None => continue
            }
        }

        "".to_owned()
    }

    fn file_from_name(full_name: &str) -> PathBuf {
        let weechat = get_weechat();
        let weechat_home = weechat.info_get("weechat_dir", "").unwrap();
        let mut file = Path::new(weechat_home.as_ref()).join("logs");
        let mut full_name = full_name.to_owned();
        full_name.push_str(".weechatlog");
        file.push(full_name);
        file
    }

    fn get_file_by_buffer(buffer: Buffer) -> Option<PathBuf> {
        let path = Ripgrep::file_from_infolist(&buffer);

        if path.is_empty() {
            let full_name = buffer.get_full_name().to_string().to_lowercase();
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

        let mut config = weechat.config_new("ripgrep", None, None);

        let section_info: ConfigSectionInfo<()> = ConfigSectionInfo {
            name: "main",
            ..Default::default()
        };

        let section = config.new_section(section_info);

        let option = section.new_boolean_option(
            "go_to_buffer",
            "Automatically go to grep buffer when search is over.",
            true,
            true,
            false,
            None,
            None::<()>
        );

        unsafe {
            _WEECHAT = Some(weechat);
        }

        Ok(Ripgrep {
            thread: None,
            fd_hook: None,
            _config: config,
            go_to_buffer_opt: option,
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
    name: "ripgrep",
    author: "Damir Jelic <poljar@termina.org.uk>",
    description: "Search in buffers and logs using ripgrep",
    version: "0.1.0",
    license: "ISC"
);
