use napo_core::Rules;

fn main() {
    let rules = Rules::default();
    println!(
        "napo-online: napo-core OK (num_players={}, hand_size={})",
        rules.num_players, rules.hand_size
    );
}
