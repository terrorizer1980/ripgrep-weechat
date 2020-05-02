mod buffer;

use clap::{App, Arg};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{channel, Sender, Receiver};

use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use std::io;
use grep_searcher::sinks::Lossy;

use std::rc::Rc;
use std::cell::RefCell;

use weechat::{
    Weechat,
    WeechatPlugin,
    ArgsWeechat,
};

use weechat::weechat_plugin;
use weechat::config::{Config, ConfigOption, ConfigSectionSettings, BooleanOptionSettings};
use weechat::hooks::{CommandHook, CommandDescription};
use weechat::buffer::Buffer;

use buffer::GrepBuffer;

type SearchResult = Result<Vec<String>, io::Error>;


struct Ripgrep {
    _config: Rc<RefCell<Config>>,
    _command: CommandHook<CommandData>,
    _runtime: Rc<RefCell<Option<Runtime>>>,
}

#[derive(Clone, Default)]
struct CommandData {
    config: Option<Rc<RefCell<Config>>>,
    buffer: Rc<RefCell<Option<GrepBuffer>>>,
    runtime: Rc<RefCell<Option<Runtime>>>,
}

impl Ripgrep {
    fn file_from_infolist(buffer: &Buffer) -> String {
        todo!()
        // let infolist = weechat.infolist_get("logger_buffer", "");

        // let infolist = match infolist {
        //     Some(list) => list,
        //     None => return "".to_owned()
        // };

        // while infolist.next() {
        //     let other_buffer = infolist.get_buffer();
        //     match other_buffer {
        //         Some(other_buffer) => {
        //             if *buffer == other_buffer {
        //                 let path = infolist.get_string("log_filename");
        //                 match path {
        //                     Some(p) => return p.to_string(),
        //                     None => continue
        //                 }
        //             }
        //         }

        //         None => continue
        //     }
        // }

        // "".to_owned()
    }

    fn file_from_name(full_name: &str) -> PathBuf {
        todo!()
        // let weechat_home = weechat.info_get("weechat_dir", "").unwrap();
        // let mut file = Path::new(weechat_home.as_ref()).join("logs");
        // let mut full_name = full_name.to_owned();
        // full_name.push_str(".weechatlog");
        // file.push(full_name);
        // file
    }

    fn get_file_by_buffer(buffer: &Buffer) -> Option<PathBuf> {
        Some(PathBuf::from_str("/home/poljar/.weechat/logs/python.termina.!awifyyvrdfpauymuut:termina.org.uk.weechatlog").expect("Can't create pathbuf from string"))
        // let path = Ripgrep::file_from_infolist(&buffer);

        // if path.is_empty() {
        //     let full_name = buffer.get_full_name().to_string().to_lowercase();
        //     return Some(Ripgrep::file_from_name(&full_name));
        // }

        // let path = PathBuf::from_str(&path);

        // match path {
        //     Ok(p) => Some(p),
        //     Err(_) => None
        // }
    }

    async fn search(
        file: PathBuf,
        matcher: RegexMatcher,
        mut sender: Sender<SearchResult>
    ) {
        let mut matches: Vec<String> = vec![];

        let sink = Lossy(|_, line| {
            matches.push(line.to_string());
            Ok(true)
        });

        match Searcher::new().search_path(&matcher, file, sink) {
            Ok(_) => sender.send(Ok(matches)),
            Err(e) => sender.send(Err(e)),
        }.await.unwrap_or(());
    }

    async fn recieve_result(command_data: CommandData, mut receiver: Receiver<SearchResult>) {
        let buffer = &command_data.buffer;
        let buffer_exists = buffer.borrow().is_some();

        if !buffer_exists {
            let buffer_handle = GrepBuffer::new(&command_data);
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
            Err(e) => {
                Weechat::print(
                    &format!("Error searching: {}", e.to_string())
                )
            }
        }

        buffer.print_status("Summary of search TODO");

        let config = command_data.config.as_ref().unwrap().borrow();
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

    fn search_command_callback(command_data: &CommandData, buffer: Buffer, args: ArgsWeechat) {
        let parsed_args = App::new("rg")
            .arg(Arg::with_name("pattern")
                               .index(1)
                               .value_name("PATTERN")
                               .help("A regular expression used for
                                     searching.")
                               .multiple(true))
            .get_matches_from_safe(args);

        let parsed_args = match parsed_args {
            Ok(a) => a,
            Err(e) => {
                Weechat::print(&format!("Error parsing grep args {}", e));
                return;
            },
        };

        let file = Ripgrep::get_file_by_buffer(&buffer);

        let file = match file {
            Some(f) => f,
            None => return
        };

        let pattern = match parsed_args.value_of("pattern") {
            Some(p) => p,
            None => {
                Weechat::print("Invalid pattern");
                return
            }
        };

        let matcher = match RegexMatcher::new(pattern) {
            Ok(m) => m,
            Err(_) => {
                Weechat::print("Invalid regex");
                return
            }
        };

        let (tx, rx) = channel(1);

        command_data.runtime.borrow_mut().as_ref().unwrap().spawn(Ripgrep::search(file, matcher, tx));
        Weechat::spawn(Ripgrep::recieve_result(command_data.clone(), rx));
    }
}

impl WeechatPlugin for Ripgrep {
    fn init(weechat: &Weechat, _args: ArgsWeechat) -> Result<Self, ()> {
        let mut config = Weechat::config_new("ripgrep").expect("Can't create ripgrep config");

        {
            let section_settings = ConfigSectionSettings::new("main");
            let mut section = config.new_section(section_settings).expect("Can't create main config section");

            let option_settings = BooleanOptionSettings::new("go_to_buffer")
                .description("Automatically go to grep buffer when search is over.")
                .default_value(true);

            section.new_boolean_option(option_settings).expect("Can't create boolean option");
        }

        let config = Rc::new(RefCell::new(config));

        let command_info = CommandDescription {
            name: "rg",
            ..Default::default()
        };

        let runtime = Rc::new(RefCell::new(Some(Runtime::new().unwrap())));

        let command_data = CommandData {
            runtime: runtime.clone(),
            buffer: Rc::new(RefCell::new(None)),
            config: Some(config.clone()),
        };

        let command = weechat.hook_command(
            command_info,
            Ripgrep::search_command_callback,
            Some(command_data),
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
