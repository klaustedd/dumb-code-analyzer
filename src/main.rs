use std::collections::HashMap;
use std::fs::{File};
use std::io::Read;
use std::{env, fs};
use regex::Regex;


#[derive(Debug)]
struct AppArguments {
    arguments : HashMap<String, String>,
    exec_path : String,
    parameters : Vec<String>,
}

impl AppArguments {
    fn new() -> AppArguments {
        // map that will store  the application arguments 
        let mut args = HashMap::<String, String>::new();

        let mut params : Vec<String> = Vec::new();

        // store the last iterated value from the application arguments
        let mut last_arg = String::new();

        // store the app execution path
        let mut exec_path = String::new();

        enum Contexts {
            ExecutionPath,
            ArgumentName,
            ArgumentValue,
            Parameter,
        }
        let mut cur_context = Contexts::ExecutionPath;

        // iterate over each application argument
        let app_args : Vec<String> = env::args().collect();
        for app_arg in app_args {

            // check if it is a special parameter
            if let Contexts::Parameter = cur_context {
                if app_arg.starts_with("-") {
                    cur_context = Contexts::ArgumentName;
                }
            }

            // add the value depending on the current context
            match cur_context {
                Contexts::ExecutionPath => {
                    exec_path = app_arg;
                    cur_context = Contexts::Parameter;
                },
                Contexts::ArgumentName => {
                    last_arg = app_arg.chars().skip(1).collect();
                    cur_context = Contexts::ArgumentValue;
                },
                Contexts::ArgumentValue => {
                    args.insert(String::from(&last_arg), app_arg);
                    cur_context = Contexts::Parameter;
                },
                Contexts::Parameter => {
                    params.push(app_arg);
                    cur_context = Contexts::Parameter;
                }
            }
        }

        // return 
        AppArguments { arguments: args, exec_path: exec_path, parameters: params }
    }
}

enum HttpVerbs {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
    Any,
}

impl HttpVerbs {
    fn as_str(&self) -> &str {

        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
            Self::Any => "ANY",

        }
    }
}

struct ControllerFileSearchResult {
    file_name: String,
    request_search_results: Vec<ControllerRequestFileSearchResult>
}

impl ControllerFileSearchResult {
    fn new(file_name : String) -> ControllerFileSearchResult {
        ControllerFileSearchResult { file_name: file_name, request_search_results: Vec::<ControllerRequestFileSearchResult>::new() }
    }
}

struct ControllerRequestFileSearchResult {
    http_verb : HttpVerbs,
    rest_path: String,
}

fn main() {
    let app_args = AppArguments::new();
    let map_dir_name = app_args.arguments.get("mapdir").expect("Expected the argument -mapdir with the location to map the directory");

    // call search_in_dir to find controllers
    let results = search_in_dir(map_dir_name);

    for result in results {
        println!("{}", result.file_name);

        for req_result in result.request_search_results {
            println!("\t{} {}", req_result.http_verb.as_str(), req_result.rest_path);
        }
    }
}

fn search_in_dir(dir_name : &str) -> Vec<ControllerFileSearchResult> {

    // read the directory entries
    let read_dir = match fs::read_dir(dir_name) {
        Ok(dirs) => dirs,
        Err(e) => panic!("Could not map 'mapdir' directory: {}", e.to_string())
    };

    // file buffer maximum size (8 MB)
    const BUFFER_SIZE : usize = 1024 * 1024 * 8;
    let file_name_regex : Regex = Regex::new(r"^.?*(Controller\.java)$").unwrap();
    let endpoint_regex : Regex = Regex::new(r"(@)\w+(Mapping).*").unwrap();

    // store the results from the searched files
    let mut file_search_results = Vec::<ControllerFileSearchResult>::new();

    for dir in read_dir {
        if let Ok(dir_entry) = dir {

            let dir_entry_path =  dir_entry.path();

            // call this function recursively if the directory entry is a directory and skip to the next iter element
            // ignore directories that starts with '.'
            if  dir_entry_path.is_dir() && 
                !dir_entry_path.file_name().unwrap().to_str().unwrap().starts_with('.') {
                file_search_results.append(&mut search_in_dir(dir_entry_path.to_str().unwrap()));
                continue;
            }

            // process the file if the directory entry is a file
            if dir_entry_path.is_file() {

                // read the file metadata
                let file_metadata = match dir_entry.metadata() {
                    Ok(m) => m,
                    Err(_) => panic!("Could not read metadata from file")
                };

                // check if file length is bigger than the maximum buffer size
                if file_metadata.len() > BUFFER_SIZE as u64 {
                    println!("Ignoring file {} because it exceeds the buffer limit of {} bytes", dir_entry_path.to_string_lossy(), BUFFER_SIZE);
                    continue;
                }

                // ignore files that does not end with *Controller.java
                if !file_name_regex.is_match(dir_entry.file_name().to_str().unwrap()) {
                    continue;
                }

                // read the file content
                let mut file = File::open(dir_entry_path).expect("Could not open file");
                let mut buf = vec![];
                match file.read_to_end(&mut buf) {
                    Err(e) => {
                        println!("Could not read file:{}. Reason:{}", dir_entry.file_name().to_str().unwrap(), e.to_string());
                        continue;
                    }
                    _ => ()
                }
                let file_data = String::from_utf8_lossy(&buf);

                let mut file_search_result = ControllerFileSearchResult::new(String::from(dir_entry.file_name().to_str().unwrap()));

                // iterate over each line of the file trying to find a match to the controller endpoints
                for line in file_data.split('\n') {


                    // check if the current line matches and endpoint declaration @[HttpVerb]Request
                    if endpoint_regex.is_match(line) && line.trim().starts_with('@') {

                        enum Contexts {
                            AnnotationName,
                            AnnotationAttributes,
                            EndpointPath,
                            EOC,
                        }

                        let mut cur_context = Contexts::AnnotationName;
                        let mut str_buffer = String::new();
                        let mut prev_c = '\0';

                        let mut endpoint_path = String::new();
                        let mut http_verb = HttpVerbs::Any;

                        for (index, cur_c) in line.chars().enumerate() {

                            // flag that indicates that the current character is escaped
                            let is_escape = prev_c == '\\';

                            match cur_context {
                                Contexts::AnnotationName => {
                                    // end of context
                                    if cur_c == '(' || index == line.len() - 1 {
                                        if str_buffer == "@RequestMapping" {
                                            http_verb = HttpVerbs::Any;
                                        }
                                        else if str_buffer == "@DeleteMapping" {
                                            http_verb = HttpVerbs::Delete;
                                        }
                                        else if str_buffer == "@GetMapping" {
                                            http_verb = HttpVerbs::Get;
                                        }
                                        else if str_buffer == "@HeadMapping" {
                                            http_verb = HttpVerbs::Head;
                                        }
                                        else if str_buffer == "@OptionsMapping" {
                                            http_verb = HttpVerbs::Options;
                                        }
                                        else if str_buffer == "@PatchMapping" {
                                            http_verb = HttpVerbs::Patch;
                                        }
                                        else if str_buffer == "@PostMapping" {
                                            http_verb = HttpVerbs::Post;
                                        }
                                        else if str_buffer == "@PutMapping" {
                                            http_verb = HttpVerbs::Put;
                                        }
                                        else {
                                            panic!("Unknown http verb annotation found:{}", str_buffer);
                                        }

                                        // clear the buffer and go to the next context
                                        str_buffer.clear();
                                        
                                        // 
                                        if cur_c == '(' {
                                            cur_context = Contexts::AnnotationAttributes
                                        }
                                    }
                                    // on context
                                    else {
                                        if !cur_c.is_whitespace() {
                                            str_buffer.push(cur_c);
                                        }
                                    }

                                },
                                Contexts::AnnotationAttributes => {
                                    if cur_c == '"' {
                                        cur_context = Contexts::EndpointPath;
                                    }
                                },
                                Contexts::EndpointPath => {
                                    // end of context
                                    if cur_c == '"' && !is_escape {
                                        endpoint_path = String::from(&str_buffer);
                                        str_buffer.clear();
                                        cur_context = Contexts::EOC;
                                    }
                                    // on context
                                    else {
                                        // ignore '\' only if not on escape character
                                        if cur_c == '\\' && !is_escape {
                                            continue;
                                        }
                                        str_buffer.push(cur_c);
                                    }
                                },
                                Contexts::EOC => ()
                            }
                            // store the current charcter as previous before going to the next iter()
                            prev_c = cur_c;
                        }

                        // on this context, the request line process is finished
                        file_search_result.request_search_results.push(
                            ControllerRequestFileSearchResult { http_verb: http_verb,  rest_path: endpoint_path }
                        )
                    }
                }                
            
                // on this context the line by line process is finished
                file_search_results.push(file_search_result);
            }
        }
    }

    file_search_results
}
