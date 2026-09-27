use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    sz_orm_studio::parse_args_and_run(env::args().collect())
}
