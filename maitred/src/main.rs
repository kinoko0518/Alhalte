use uuid::Uuid;

pub mod banquette;
pub mod cellar;

type UserID = Uuid;
type SessionID = Uuid;

type RoomID = Uuid;

enum MaitreDEvents {}

fn main() {
    println!("Hello, world!");
}
