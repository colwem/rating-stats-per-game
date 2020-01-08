use std::error::Error;
use pgn_reader::{Visitor, RawHeader, Skip, BufferedReader};
use std::fs::File;
use histogram::Histogram;
use std::io::{self, Write, Read};
use clap::{Arg, App};
use regex::Regex;
use std::ops::Range;

#[macro_use] 
extern crate lazy_static;

struct RatingPool {
    histogram: Histogram,
    perf_type: PerfType
}

struct PerfType {
    name: &'static str,
    speed: Range<u64>
}

struct Ratings {
    rating_pools: Vec<RatingPool>,
    pool: Option<usize>,
    white_rating: Option<u64>,
    black_rating: Option<u64>,
    games_skipped: usize,
    casual: usize,
    rated: bool,
}

impl Default for Ratings {
    fn default() -> Self {
        Ratings::new(
            Box::new([PerfType {name: "ultrabullet", speed: 0..30}, 
            PerfType {name: "bullet", speed: 30..180},
            PerfType {name: "blitz", speed: 180..480},
            PerfType {name: "rapid", speed: 480..1500},
            PerfType {name: "classical", speed: 1500..21600}]))
    }
}

impl Ratings {
    fn new(perf_types: Box<[PerfType]>) -> Ratings {
        let mut rating_pools: Vec<RatingPool> = Vec::new();
        for perf_type in perf_types.into_iter() {
            rating_pools.push(
                RatingPool { 
                    histogram: Histogram::configure()
                        .max_value(3500)
                        .build()
                        .unwrap(),
                    perf_type: perf_type
                });
        }
        
        Ratings { 
            rating_pools: rating_pools,
            pool: None,
            white_rating: None,
            black_rating: None,
            games_skipped: 0,
            casual: 0,
            rated: false,
        }
    }


    fn set_pool(&mut self, timecontrol_text: &str) {
        lazy_static! {
            static ref re: Regex = Regex::new(r"(?P<initial>\d+)\+(?P<increment>\d+)").unwrap();
        }
        if let Some(caps) = re.captures(timecontrol_text) {
            let initial = caps
                .name("initial").unwrap().as_str()
                .parse::<u64>().unwrap();
            let increment = caps
                .name("increment").unwrap().as_str()
                .parse::<u64>().unwrap();
            let total = initial + 40 * increment;
            for (i, pool) in self.rating_pools.into_iter().enumerate() {
                if pool.perf_type.speed.contains(&total) {
                    self.pool = Some(i);
                    return;
                }
            }
        }
        println!("Skipped");
        println!("{}", timecontrol_text);
        self.pool = None;
    }

    fn set_rated(&mut self, event: &str) {
        let event_lower = event.to_lowercase();
        let casual_words = ["casual", "simul"];
        for word in &casual_words {
            if event.contains(word) {
                self.rated = false;
                self.casual += 1;
                return
            }
        }
        self.rated = true;
    }

    fn set_black_rating(&mut self, rating: &str) {
        self.black_rating = Ratings::parse_rating(rating);
    }

    fn set_white_rating(&mut self, rating: &str) {
        self.black_rating = Ratings::parse_rating(rating);
    }

    fn parse_rating(rating: &str) -> Option<u64> {
        let new_rating: String = rating.chars()
            .filter(|c| c.is_numeric())
            .collect();
        println!("parsing: before {}, after {}", rating, new_rating);
        return new_rating.parse::<u64>().ok();
    }
}

impl Visitor for Ratings {
    type Result = ();

    fn begin_headers(&mut self) {
        self.pool = None;
        self.white_rating = None;
        self.black_rating = None;
    }

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        match key {
            b"Event" => self.set_rated(&value.decode_utf8().unwrap()),
            b"WhiteElo" => self.set_white_rating(&value.decode_utf8().unwrap()),
            b"BlackElo" => self.set_black_rating(&value.decode_utf8().unwrap()),
            b"TimeControl" => self.set_pool(&value.decode_utf8().unwrap()),
            _ => ()
        }
    }

    fn end_headers(&mut self) -> Skip {
        match self.pool {
            Some(pool) =>  {
                if let Some(rating) = self.white_rating {
                    self.rating_pools[pool]
                        .histogram.increment(rating).unwrap();
                }
                if let Some(rating) = self.black_rating {
                    self.rating_pools[pool]
                        .histogram.increment(rating).unwrap();
                }
            },
            None => self.games_skipped = self.games_skipped + 1
        }
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {
        ()
    }

}


fn main() -> Result<(), Box<dyn Error>> {

    let matches = App::new("Pgn thing")
        .version("0.0.0")
        .author("Martin Colwell <colwem@gmail.com>")
        .arg(Arg::with_name("file")
             .short("f")
             .long("file")
             .takes_value(true)
             .help("A pgn file"))
        .get_matches();

    let input: Box<dyn Read> = match matches.value_of("file") {
        Some(file_name) => Box::new(File::open(file_name)?),
        None => Box::new(io::stdin())
    };
    let mut reader = BufferedReader::new(input);
    let mut ratings = Ratings::default();
    reader.read_all(&mut ratings)?;
    for rating_pool in ratings.rating_pools.into_iter() {
        println!("{}: total {}, mean {}, stddev {}", 
                 rating_pool.perf_type.name, 
                 rating_pool.histogram.entries(),
                 rating_pool.histogram.mean()
                     .and_then(|v| Ok(v.to_string()))
                     .unwrap_or(String::from("No games")),
                 rating_pool.histogram.stddev()
                     .and_then(|v| Some(v.to_string()))
                     .unwrap_or(String::from("No games")));

        let mut out: File = File::create(format!("{}.csv", rating_pool.perf_type.name)).unwrap();
        rating_pool.histogram.into_iter()
            .filter(|bin| bin.count() > 0)
            .for_each(|bin| {
                out.write(format!("{},{}\n", bin.value(), bin.count()).as_bytes()).unwrap();
            });
    }
    println!("Skipped: {}", ratings.games_skipped);
    println!("Casual: {}", ratings.casual);
    Ok(())
}
