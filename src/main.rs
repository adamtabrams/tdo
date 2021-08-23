use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version};
use clap::{AppSettings, Arg, ArgMatches, SubCommand};
use colored::Colorize;
use skim::prelude::*;
use std::cmp::Ordering;
use std::env;
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::{BufWriter, Cursor, Error, ErrorKind, Read};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::process::Command;

// include_str!("../Cargo.toml");

fn main() -> Result<(), Error> {
    // setup subcommands and args
    let matches = app_from_crate!()
        .settings(&[
            AppSettings::ArgsNegateSubcommands,
            AppSettings::ColoredHelp,
            AppSettings::DisableHelpSubcommand,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
        ])
        .arg(
            Arg::with_name("default_file")
                .env("TDO_DEFAULT_FILE")
                .help("Path to use when current directory has no todo file")
                .hide_env_values(true),
        )
        .arg(Arg::with_name("editor_env").env("EDITOR").hidden(true))
        .subcommand(
            SubCommand::with_name("view")
                .visible_alias("v")
                .about(format!("{:<30}", "Show existing tasks").as_str()),
        )
        .subcommand(
            SubCommand::with_name("add")
                .visible_alias("a")
                .about(format!("{:<30}", "Add new tasks").as_str()),
        )
        .subcommand(
            SubCommand::with_name("remove")
                .visible_alias("r")
                .about(format!("{:<30}", "Select tasks to remove").as_str()),
        )
        .subcommand(
            SubCommand::with_name("set")
                .visible_alias("s")
                .about(format!("{:<30}", "Change status of tasks").as_str()),
        )
        .subcommand(
            SubCommand::with_name("modify")
                .visible_alias("m")
                .about(format!("{:<30}", "Change text of tasks").as_str()),
        )
        .subcommand(
            SubCommand::with_name("editor")
                .visible_alias("e")
                .about(format!("{:<30}", "Open tasks with EDITOR").as_str()),
        )
        .subcommand(SubCommand::with_name("sort").about("Sort tasks by status"))
        // .subcommand(SubCommand::with_name("clean").about("Delete completed tasks"))
        .get_matches();

    // execute options

    // set path
    let path = get_path(&matches)?;

    // read file
    let lines = read_file(&path)?;
    let mut tasks = Tasks::new(lines);

    // execute subcommands
    let user_commands = [
        UserCommand {
            name: "view".to_string(),
            func: Box::new(|t, _, _| user_view(t)),
        },
        UserCommand {
            name: "add".to_string(),
            func: Box::new(|t, p, _| user_add(t, p)),
        },
        UserCommand {
            name: "remove".to_string(),
            func: Box::new(|t, p, _| user_remove(t, p)),
        },
        UserCommand {
            name: "set".to_string(),
            func: Box::new(|t, p, _| user_set(t, p)),
        },
        UserCommand {
            name: "modify".to_string(),
            func: Box::new(|t, p, _| user_modify(t, p)),
        },
        UserCommand {
            name: "editor".to_string(),
            func: Box::new(|_, p, m| user_editor(m, p)),
        },
        UserCommand {
            name: "sort".to_string(),
            func: Box::new(|t, p, _| user_sort(t, p)),
        },
    ];

    for c in &user_commands {
        if matches.subcommand_matches(c.name.as_str()).is_some() {
            return (c.func)(&mut tasks, &path, &matches);
        }
    }

    // TODO implement clean
    // if matches.subcommand_matches("clean").is_some() {
    //     tasks.sort();
    //     write_file(path, tasks.to_file())?;
    //     return Ok(());
    // }

    // TODO implement interactive
    // TODO implement init if none exists
    while tasks.interactive(&path, &matches, &user_commands)? {}

    Ok(())
}

fn user_view(tasks: &mut Tasks) -> Result<(), Error> {
    tasks.sort();
    println!("\n{}", tasks);
    Ok(())
}

// consider setting status
fn user_add(tasks: &mut Tasks, path: &Path) -> Result<(), Error> {
    let mut is_modified = false;

    while let Some(new_task) = Task::from_user(tasks.len()) {
        tasks.add(new_task);
        is_modified = true;
    }

    if is_modified {
        write_file(path, tasks.to_file())?;
    }

    Ok(())
}

fn user_remove(tasks: &mut Tasks, path: &Path) -> Result<(), Error> {
    while tasks.select("remove > ", Tasks::delete_task) {}
    write_file(path, tasks.to_file())?;
    Ok(())
}

fn user_set(tasks: &mut Tasks, path: &Path) -> Result<(), Error> {
    while tasks.select("set > ", Tasks::set_status) {}
    write_file(path, tasks.to_file())?;
    Ok(())
}

fn user_modify(tasks: &mut Tasks, path: &Path) -> Result<(), Error> {
    while tasks.select("modify > ", Tasks::set_text) {}
    write_file(path, tasks.to_file())?;
    Ok(())
}

fn user_editor(matches: &ArgMatches, path: &Path) -> Result<(), Error> {
    let editor: &str;
    match matches.value_of("editor_env") {
        Some(e) => editor = e,
        _ => editor = "vi",
    }
    Command::new(editor).arg(path).status()?;
    Ok(())
}

fn user_sort(tasks: &mut Tasks, path: &Path) -> Result<(), Error> {
    tasks.sort();
    write_file(path, tasks.to_file())?;
    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<String>, Error> {
    let mut file = File::open(path)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(buf.lines().map(|l| l.to_string()).collect())
}

fn write_file(path: &Path, text: String) -> Result<(), Error> {
    let file = OpenOptions::new().write(true).truncate(true).open(path)?;
    let mut buf = BufWriter::new(file);
    buf.write_all(text.as_bytes())?;
    buf.flush()?;
    Ok(())
}

fn get_path(matches: &ArgMatches) -> Result<PathBuf, Error> {
    const LOCAL_PATH: &str = ".todo.md";
    let mut full_path = env::current_dir()?;

    if Path::new(LOCAL_PATH).is_file() {
        full_path.push(LOCAL_PATH);

        return Ok(full_path);
    } else if let Some(default_path) = matches.value_of("default_file") {
        println!("using default file: {}", default_path);

        if Path::new(default_path).is_file() {
            return Ok(PathBuf::from(default_path));
        } else {
            return Err(Error::new(
                ErrorKind::NotFound,
                "path does not lead to a valid file",
            ));
        }
    }

    Err(Error::new(
        ErrorKind::NotFound,
        format!(
            "file {} not found in current directory and no default is set",
            LOCAL_PATH
        ),
    ))
}

struct Tasks(Vec<Task>);

#[derive(Eq)]
struct Task {
    id: usize,
    text: String,
    status: Status,
}

#[derive(Eq)]
enum Status {
    Todo,
    Done,
    Other,
}

struct UserCommand {
    name: String,
    func: Box<dyn Fn(&mut Tasks, &Path, &ArgMatches) -> Result<(), Error>>,
}

impl Tasks {
    fn new(lines: Vec<String>) -> Self {
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| Task::parse(i + 1, l))
            .collect()
    }

    fn sort(&mut self) {
        self.0.sort();
    }

    fn len(&mut self) -> usize {
        self.0.len()
    }

    fn iter(&self) -> std::slice::Iter<Task> {
        self.0.iter()
    }

    fn iter_mut(&mut self) -> std::slice::IterMut<Task> {
        self.0.iter_mut()
    }

    fn add(&mut self, task: Task) {
        self.0.push(task)
    }

    fn remove(&mut self, index: usize) -> Task {
        self.0.remove(index)
    }

    fn index_of(&self, id: usize) -> Option<usize> {
        for (i, t) in self.iter().enumerate() {
            if t.id == id {
                return Some(i);
            }
        }
        None
    }

    fn delete_id(&mut self, id: usize) -> Option<Task> {
        if let Some(index) = self.index_of(id) {
            return Some(self.remove(index));
        }
        None
    }

    fn get_id(&mut self, id: usize) -> Option<&mut Task> {
        self.iter_mut().find(|t| t.id == id)
    }

    fn select(&mut self, prompt: &str, f: fn(&mut Self, usize)) -> bool {
        let buf = Cursor::new(self.to_string());
        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(buf);
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .prompt(Some(prompt))
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            if !out.is_abort {
                if let Some(item) = out.selected_items.get(0) {
                    if let Some(id) = Task::parse_id(&item.output().to_string()) {
                        f(self, id);
                        return true;
                    }
                }
            }
        }

        false
    }

    fn set_status(&mut self, id: usize) {
        let task = match self.get_id(id) {
            Some(t) => t,
            None => return,
        };

        let done = format!("{} done", "✓".green());
        let todo = format!("{} todo", "x".red());
        let other = format!("{} other", "~".yellow());

        let status_list = format!("{}\n{}\n{}\n", done, todo, other);
        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(Cursor::new(status_list));
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            if !out.is_abort {
                if let Some(item) = out.selected_items.get(0) {
                    let i = item.output().to_string();
                    if i.contains("done") {
                        task.status = Status::Done;
                    }
                    if i.contains("todo") {
                        task.status = Status::Todo;
                    }
                    if i.contains("other") {
                        task.status = Status::Other;
                    }
                }
            }
        }
    }

    fn set_text(&mut self, id: usize) {
        let task = match self.get_id(id) {
            Some(t) => t,
            None => return,
        };

        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(Cursor::new(""));
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .prompt(Some("new text: "))
            .query(Some(task.text.as_str()))
            .color(Some("info:8,bg:8"))
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            let new_text = out.query;

            if !out.is_abort && !new_text.is_empty() {
                task.text = new_text;
            }
        }
    }

    fn delete_task(&mut self, id: usize) {
        let task = match self.get_id(id) {
            Some(t) => t,
            None => return,
        };

        let answer_list = "no\nyes\n";
        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(Cursor::new(answer_list));
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .prompt(Some("permanently delete task: "))
            .header(Some(task.text.as_str()))
            .color(Some("info:8,bg:8"))
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            if !out.is_abort {
                if let Some(item) = out.selected_items.get(0) {
                    let answer = item.output().to_string();
                    if answer == "yes" {
                        self.delete_id(id);
                    }
                }
            }
        }
    }

    fn to_file(&self) -> String {
        self.iter()
            .map(|t| t.to_file())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn interactive(
        &mut self,
        path: &Path,
        matches: &ArgMatches,
        user_commands: &[UserCommand],
    ) -> Result<bool, Error> {
        let preview_path = format!("{:?} view", env::current_exe()?);
        let command_names = user_commands
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(Cursor::new(command_names));
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .preview(Some(preview_path.as_str()))
            .preview_window(Some("right:80%"))
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            if !out.is_abort {
                if let Some(item) = out.selected_items.get(0) {
                    for c in user_commands {
                        if item.output().to_string().as_str() == c.name {
                            (c.func)(self, path, matches)?;
                            return Ok(true);
                        }
                    }
                }
            }
        }

        Ok(false)
    }
}

impl Display for Tasks {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        for task in self.iter() {
            f.write_fmt(format_args!("{}\n", task))?;
        }
        Ok(())
    }
}

impl FromIterator<Task> for Tasks {
    fn from_iter<I: IntoIterator<Item = Task>>(iter: I) -> Self {
        let mut t = Tasks(Vec::new());

        for i in iter {
            t.0.push(i);
        }

        t
    }
}

impl Task {
    fn parse(id: usize, line: &str) -> Self {
        let text;

        if line.starts_with('-') {
            text = line.replacen("-", "", 1).trim().to_string();
        } else {
            text = line.trim().to_string();
        }

        if text.starts_with("[ ]") {
            return Task {
                id,
                text: text.replacen("[ ]", "", 1).trim().to_string(),
                status: Status::Todo,
            };
        }

        if text.starts_with("[x]") {
            return Task {
                id,
                text: text.replacen("[x]", "", 1).trim().to_string(),
                status: Status::Done,
            };
        }

        Task {
            id,
            text,
            status: Status::Other,
        }
    }

    fn parse_id(line: &str) -> Option<usize> {
        if let Some(num) = line.trim().split_whitespace().next() {
            return num.parse().ok();
        }
        None
    }

    fn to_file(&self) -> String {
        match self {
            Task {
                status: Status::Todo,
                text,
                ..
            } => format!("- [ ] {}", text),
            Task {
                status: Status::Done,
                text,
                ..
            } => format!("- [x] {}", text),
            Task {
                status: Status::Other,
                text,
                ..
            } => format!("- {}", text),
        }
    }

    fn from_user(len: usize) -> Option<Self> {
        let reader_option = SkimItemReaderOption::default().ansi(true).build();
        let skim_reader = SkimItemReader::new(reader_option).of_bufread(Cursor::new(""));
        let skim_config = SkimOptionsBuilder::default()
            .height(Some("50%"))
            .reverse(true)
            .prompt(Some("new task text: "))
            .color(Some("info:8,bg:8"))
            .build()
            .unwrap();

        let skim_output = Skim::run_with(&skim_config, Some(skim_reader));

        if let Some(out) = skim_output {
            let new_text = out.query;

            if !out.is_abort && !new_text.is_empty() {
                return Some(Task {
                    id: len + 2,
                    text: new_text,
                    status: Status::Todo,
                });
            }
        }

        None
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        colored::control::set_override(true);
        match self {
            Task {
                id,
                text,
                status: Status::Todo,
            } => f.write_fmt(format_args!("{:>5} | {} {}", id, "✕".red(), text)),
            Task {
                id,
                text,
                status: Status::Done,
            } => f.write_fmt(format_args!("{:>5} | {} {}", id, "✓".green(), text)),
            Task {
                id,
                text,
                status: Status::Other,
            } => f.write_fmt(format_args!("{:>5} | {}", id, text)),
        }
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Task {
                    id: ia, status: sa, ..
                },
                Task {
                    id: ib, status: sb, ..
                },
            ) if sa == sb => ia.cmp(ib),
            (Task { status: a, .. }, Task { status: b, .. }) => a.cmp(b),
        }
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.status == other.status && self.id == other.id && self.text == other.text
    }
}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Status::Todo, _) => Ordering::Less,
            (Status::Done, Status::Other) => Ordering::Less,
            (Status::Done, Status::Done) => Ordering::Equal,
            (Status::Done, Status::Todo) => Ordering::Greater,
            (Status::Other, _) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Status {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Status::Todo, Status::Todo)
                | (Status::Done, Status::Done)
                | (Status::Other, Status::Other)
        )
    }
}
