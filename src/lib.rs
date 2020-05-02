mod buffer;

use clap::{App, Arg};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use grep_regex::RegexMatcher;
use grep_searcher::sinks::Lossy;
use grep_searcher::Searcher;
use std::io;

use std::cell::RefCell;
use std::rc::Rc;

use weechat::{infolist::InfolistVariable, ArgsWeechat, Weechat, WeechatPlugin};

use weechat::buffer::{Buffer, BufferInputCallback};
use weechat::config::{BooleanOptionSettings, Config, ConfigOption, ConfigSectionSettings};
use weechat::hooks::{Command, CommandCallback, CommandSettings};
use weechat::weechat_plugin;

use buffer::GrepBuffer;

type SearchResult = Result<Vec<String>, io::Error>;

struct Ripgrep {
    _config: Rc<RefCell<Config>>,
    _command: Command,
    _runtime: Rc<RefCell<Option<Runtime>>>,
}

#[derive(Clone)]
struct RipgrepCommand {
    config: Rc<RefCell<Config>>,
    buffer: Rc<RefCell<Option<GrepBuffer>>>,
    runtime: Rc<RefCell<Option<Runtime>>>,
    last_search_file: Rc<RefCell<Option<PathBuf>>>,
}

impl RipgrepCommand {
    async fn receive_result(&self, mut receiver: Receiver<SearchResult>) {
        let buffer = &self.buffer;
        let buffer_exists = buffer.borrow().is_some();

        if !buffer_exists {
            let buffer_handle = GrepBuffer::new(&self);
            *buffer.borrow_mut() = Some(buffer_handle);
        }

        let buffer_borrow = buffer.borrow();
        let buffer = buffer_borrow.as_ref().expect("Buffer wasn't created");

        buffer.print_status("Search for TODO");
        let result = receiver.recv().await;

        let result = if let Some(result) = result {
            result
        } else {
            Weechat::print(&format!("Error searching: empty result"));
            return;
        };

        match result {
            Ok(result) => {
                for line in result {
                    buffer.print(&line);
                }
            }
            Err(e) => Weechat::print(&format!("Error searching: {}", e.to_string())),
        }

        buffer.print_status("Summary of search TODO");

        let config = self.config.borrow();
        let section = config.search_section("main").unwrap();
        let go_to_buffer = section.search_option("go_to_buffer").unwrap();

        let go_to_buffer = match go_to_buffer {
            ConfigOption::Boolean(opt) => opt,
            _ => panic!("Invalid option type"),
        };

        if go_to_buffer.value() {
            buffer.switch_to();
        }
    }

    async fn receive_result_helper(command: RipgrepCommand, rx: Receiver<SearchResult>) {
        command.receive_result(rx).await
    }

    fn file_from_infolist(&self, weechat: &Weechat, buffer: &Buffer) -> Option<String> {
        let mut infolist = weechat.get_infolist("logger_buffer", None).ok()?;

        while let Some(item) = infolist.next() {
            let info_buffer = if let Some(b) = item.get("buffer") {
                b
            } else {
                continue;
            };

            if let InfolistVariable::Buffer(info_buffer) = info_buffer {
                if buffer == &info_buffer {
                    let path = item.get("log_filename")?;

                    if let InfolistVariable::String(path) = path {
                        return Some(path.to_string());
                    }
                }
            }
        }

        None
    }

    fn file_from_name(&self, full_name: &str) -> PathBuf {
        let weechat_home = Weechat::info_get("weechat_dir", "").expect("Can't find Weechat home");
        let mut file = Path::new(&weechat_home).join("logs");
        let mut full_name = full_name.to_owned();
        full_name.push_str(".weechatlog");
        file.push(full_name);
        file
    }

    fn get_file_by_buffer(&self, weechat: &Weechat, buffer: &Buffer) -> Option<PathBuf> {
        let path = self.file_from_infolist(weechat, buffer);

        if let Some(path) = path {
            PathBuf::from_str(&path)
        } else {
            let full_name = buffer.full_name().to_lowercase();
            Ok(self.file_from_name(&full_name))
        }
        .ok()
    }

    async fn search(file: PathBuf, matcher: RegexMatcher, mut sender: Sender<SearchResult>) {
        let mut matches: Vec<String> = vec![];

        let sink = Lossy(|_, line| {
            matches.push(line.to_string());
            Ok(true)
        });

        match Searcher::new().search_path(&matcher, file, sink) {
            Ok(_) => sender.send(Ok(matches)),
            Err(e) => sender.send(Err(e)),
        }
        .await
        .unwrap_or(());
    }
}

impl BufferInputCallback for RipgrepCommand {
    fn callback(&mut self, weechat: &Weechat, buffer: &Buffer, input: Cow<str>) -> Result<(), ()> {
        let file = self.get_file_by_buffer(weechat, buffer);

        let file = match file {
            Some(f) => f,
            None => return Err(()),
        };

        let matcher = match RegexMatcher::new(&input) {
            Ok(m) => m,
            Err(e) => {
                buffer.print(&format!(
                    "{} Invalid regular expression {:?}",
                    Weechat::prefix("error"),
                    e
                ));
                return Err(());
            }
        };

        let (tx, rx) = channel(1);

        self.runtime
            .borrow_mut()
            .as_ref()
            .unwrap()
            .spawn(RipgrepCommand::search(file, matcher, tx));
        Weechat::spawn(RipgrepCommand::receive_result_helper(self.clone(), rx));

        Ok(())
    }
}

impl CommandCallback for RipgrepCommand {
    fn callback(&mut self, weechat: &Weechat, buffer: &Buffer, arguments: ArgsWeechat) {
        let parsed_args = App::new("rg")
            .arg(
                Arg::with_name("pattern")
                    .index(1)
                    .value_name("PATTERN")
                    .help("A regular expression used for searching.")
                    .multiple(true),
            )
            .get_matches_from_safe(arguments);

        let parsed_args = match parsed_args {
            Ok(a) => a,
            Err(e) => {
                Weechat::print(&format!("Error parsing grep args {}", e));
                return;
            }
        };

        let file = self.get_file_by_buffer(weechat, buffer);

        let file = match file {
            Some(f) => f,
            None => return,
        };

        let pattern = match parsed_args.value_of("pattern") {
            Some(p) => p,
            None => {
                Weechat::print("Invalid pattern");
                return;
            }
        };

        let matcher = match RegexMatcher::new(pattern) {
            Ok(m) => m,
            Err(_) => {
                Weechat::print("Invalid regex");
                return;
            }
        };

        let (tx, rx) = channel(1);

        self.runtime
            .borrow_mut()
            .as_ref()
            .unwrap()
            .spawn(RipgrepCommand::search(file, matcher, tx));
        Weechat::spawn(RipgrepCommand::receive_result_helper(self.clone(), rx));
    }
}

impl WeechatPlugin for Ripgrep {
    fn init(weechat: &Weechat, _args: ArgsWeechat) -> Result<Self, ()> {
        let mut config = Weechat::config_new("ripgrep").expect("Can't create ripgrep config");

        {
            let section_settings = ConfigSectionSettings::new("main");
            let mut section = config
                .new_section(section_settings)
                .expect("Can't create main config section");

            let option_settings = BooleanOptionSettings::new("go_to_buffer")
                .description("Automatically go to grep buffer when search is over.")
                .default_value(true);

            section
                .new_boolean_option(option_settings)
                .expect("Can't create boolean option");
        }

        let config = Rc::new(RefCell::new(config));

        let command_info = CommandSettings::new("rg");

        let runtime = Rc::new(RefCell::new(Some(Runtime::new().unwrap())));

        let command = weechat.hook_command(
            command_info,
            RipgrepCommand {
                runtime: runtime.clone(),
                buffer: Rc::new(RefCell::new(None)),
                config: config.clone(),
                last_search_file: Rc::new(RefCell::new(None)),
            },
        );

        Ok(Ripgrep {
            _config: config,
            _command: command,
            _runtime: runtime,
        })
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
