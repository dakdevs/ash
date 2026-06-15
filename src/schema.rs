diesel::table! {
    context_events (id) {
        id -> Integer,
        created_at_ms -> BigInt,
        kind -> Text,
        body -> Text,
    }
}
