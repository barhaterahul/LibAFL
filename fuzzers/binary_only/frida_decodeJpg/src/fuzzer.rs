use std::{cell::RefCell, path::PathBuf, rc::Rc};

use frida_gum::Gum;
use libafl::{
    corpus::{CachedOnDiskCorpus, Corpus, OnDiskCorpus},
    events::{
        launcher::Launcher, llmp::LlmpRestartingEventManager, ClientDescription, EventConfig,
    },
    executors::{inprocess::InProcessExecutor, ExitKind, ShadowExecutor},
    feedback_or, feedback_or_fast,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback, TimeoutFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    inputs::{BytesInput, HasTargetBytes},
    monitors::MultiMonitor,
    mutators::{
        havoc_mutations::havoc_mutations,
        scheduled::{tokens_mutations, HavocScheduledMutator},
        token_mutations::{I2SRandReplace, Tokens},
    },
    observers::{CanTrack, HitcountsMapObserver, StdMapObserver, TimeObserver},
    schedulers::{IndexesLenTimeMinimizerScheduler, QueueScheduler},
    stages::{IfElseStage, ShadowTracingStage, StdMutationalStage},
    state::{HasCorpus, StdState},
    Error, HasMetadata,
};
use libafl_bolts::{
    cli::{parse_args, FuzzerOptions},
    rands::StdRand,
    shmem::{ShMemProvider, StdShMemProvider},
    tuples::{tuple_list, Merge},
    AsSlice,
};
use libafl_frida::{
    asan::{
        asan_rt::AsanRuntime,
        errors::{AsanErrorsFeedback, AsanErrorsObserver},
    },
    cmplog_rt::CmpLogRuntime,
    coverage_rt::{CoverageRuntime, MAP_SIZE},
    executor::FridaInProcessExecutor,
    frida_helper_shutdown_observer::FridaHelperObserver,
    helper::{FridaInstrumentationHelper, IfElseRuntime},
};
use libafl_targets::cmplog::CmpLogObserver;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub fn main() {
    env_logger::init();
    color_backtrace::install();
    let options = parse_args();

    log::info!("Frida fuzzer starting up.");
    match fuzz(&options) {
        Ok(()) | Err(Error::ShuttingDown) => println!("\nFinished fuzzing. Good bye."),
        Err(e) => panic!("Error during fuzzing: {e:?}"),
    }
}