use std::error::Error;
use pgn_reader::{Visitor, RawHeader, Skip, BufferedReader};
use std::env;
use std::fs::File;
use histogram::Histogram;
use std::convert::From;
use std::io::Write;

#[derive(Default)]
struct RatingPool {
    histogram: Histogram,
    name: &'static str
}

struct Ratings<'a> {
    rating_pools: Vec<RatingPool>,
    pool: Option<&'a RatingPool>,
    white_rating: Option<u64>,
    black_rating: Option<u64>
}

impl<'a> Default for Ratings<'a> {
    fn default() -> Self {
        Ratings::new(&["ultrabullet", "bullet", "blitz", "rapid", "classical"])
    }
}

impl<'a> Ratings<'a> {
    fn new(pool_names: &[&'static str]) -> Ratings<'a> {
        let rating_pools: Vec<RatingPool> = pool_names.iter()
            .map(|name| RatingPool { name, ..Default::default() })
            .collect();
        
        Ratings { 
            rating_pools: rating_pools,
            pool: None,
            white_rating: None,
            black_rating: None
        }
    }

    fn set_pool(&'a mut self, event_text: &str) {
        let event_text_lower = event_text.to_lowercase();
        for pool in &self.rating_pools {
            if event_text_lower.contains(pool.name) {
                self.pool = Some(&pool);
                return ();
            }
        }
        self.pool = None;
    }

    fn set_black_rating(&mut self, rating: &str) {
        self.black_rating = Ratings::parse_rating(rating);
    }

    fn set_white_rating(&mut self, rating: &str) {
        self.black_rating = Ratings::parse_rating(rating);
    }

    fn parse_rating(rating: &str) -> Option<u64> {
        if rating.ends_with("?") {
            return None;
        }
        return rating.parse::<u64>().ok();
    }
}

impl<'a> Visitor for Ratings<'a> {
    type Result = ();

    fn begin_headers(&mut self) {
        self.pool = None;
        self.white_rating = None;
        self.black_rating = None;
    }

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        match key {
            b"Event" => self.set_pool(&value.decode_utf8().unwrap()),
            b"White Rating" => self.set_white_rating(&value.decode_utf8().unwrap()),
            b"Black Rating" => self.set_black_rating(&value.decode_utf8().unwrap()),
            _ => ()
        }
    }

    fn end_headers(&mut self) -> Skip {
        match &self.pool {
            Some(pool) => {
                if let Some(rating) = self.white_rating {
                    pool.histogram.increment(rating).unwrap();
                }
                if let Some(rating) = self.black_rating {
                    pool.histogram.increment(rating).unwrap();
                }
            },
            None => ()
        }
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {
        ()
    }

}


fn main() -> Result<(), Box<dyn Error>> {

    let args: Vec<String> = env::args().collect();
    let filename = &args[1];
    println!("{}", filename);
    let file = File::open(filename)?;
    let mut reader = BufferedReader::new(file);
    let mut ratings = Ratings::default();
    reader.read_all(&mut ratings)?;
    for rating_pool in ratings.rating_pools {
        println!("{}: mean {}, stddev {}", 
                 rating_pool.name, 
                 rating_pool.histogram.mean().unwrap(), 
                 rating_pool.histogram.stddev().unwrap());

        let mut out: File = File::create(format!("{}.data", rating_pool.name)).unwrap();
        for bin in rating_pool.histogram.into_iter() {
            out.write(format!("{}, {}", bin.value(), bin.count()).as_bytes()).unwrap();
        }
    }
    Ok(())
}
