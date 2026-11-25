//! Picks a resting action for free time: sit, walk, play, or run. Some come up more often
//! than others.

use fastrand::Rng;

/// Something the cat does for a while in free time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Still, with a blink or a tail flick.
    Sit,
    /// A stroll to a new spot in the bar.
    Walk,
    /// Bats at its ball, or pounces.
    Play,
    /// A dash from one end of the bar to the other.
    Run,
}

/// How often each action comes up, out of the sum of them all. Sit is most of the time.
const WEIGHTS: [(Action, u32); 4] = [
    (Action::Sit, 5),
    (Action::Walk, 3),
    (Action::Play, 1),
    (Action::Run, 1),
];

/// Picks the next action, with `rng` deciding.
pub fn pick(rng: &mut Rng) -> Action {
    let total: u32 = WEIGHTS.iter().map(|(_, weight)| weight).sum();
    let mut roll = rng.u32(0..total);
    for (action, weight) in WEIGHTS {
        if roll < weight {
            return action;
        }
        roll -= weight;
    }
    // Not reached: `roll` is less than the sum of the weights.
    Action::Sit
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn every_action_comes_up_and_sit_comes_up_most() {
        let mut rng = Rng::with_seed(7);
        let mut counts: HashMap<Action, u32> = HashMap::new();
        for _ in 0..1000 {
            *counts.entry(pick(&mut rng)).or_default() += 1;
        }

        let count = |action| counts.get(&action).copied().unwrap_or(0);
        assert!(WEIGHTS.iter().all(|&(action, _)| count(action) > 0));
        assert!(count(Action::Sit) > count(Action::Walk));
        assert!(count(Action::Walk) > count(Action::Run));
    }
}
