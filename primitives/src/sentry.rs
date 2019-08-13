pub trait SentryInterface {
    fn propagate() -> bool;
    fn get_latest_msg -> None;
    fn get_our_latest_msg -> None;
    fn get_last_approve -> None;
    fn get_last_msgs ->None;
    fn get_event_aggrs -> None;
}