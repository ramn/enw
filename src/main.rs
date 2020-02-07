use enw::BoxError;

fn main() -> Result<(), BoxError> {
    enw::run(std::env::args())
}
