/// Returns payouts sorted descending, padded or truncated to `player_count` entries.
pub(crate) fn normalize_payouts(payouts: &[i32], player_count: usize) -> Vec<i32> {
    let mut normalized = payouts.to_vec();
    normalized.sort_unstable_by(|a, b| b.cmp(a));
    normalized.resize(player_count, 0);
    normalized.truncate(player_count);
    normalized
}

/// Computes exact Malmuth-Harville ICM values for terminal stacks.
///
/// Zero-stack players are treated as already busted and split the remaining bottom payouts equally.
/// Positive-stack players compete for the top payouts according to their chip shares.
pub(crate) fn terminal_icm_values(stacks: &[f64], payouts: &[i32]) -> Result<Vec<f64>, String> {
    if stacks.is_empty() {
        return Err("ICM stacks must not be empty".to_string());
    }

    for &stack in stacks {
        if !stack.is_finite() || stack < 0.0 {
            return Err("ICM stacks must be finite and non-negative".to_string());
        }
    }

    let payouts = normalize_payouts(payouts, stacks.len());
    let positive_indices = stacks
        .iter()
        .enumerate()
        .filter_map(|(index, &stack)| (stack > 0.0).then_some(index))
        .collect::<Vec<_>>();

    if positive_indices.is_empty() {
        let average =
            payouts.iter().map(|&payout| payout as f64).sum::<f64>() / stacks.len() as f64;
        return Ok(vec![average; stacks.len()]);
    }

    let num_positive = positive_indices.len();
    let busted_value = if num_positive < stacks.len() {
        payouts[num_positive..]
            .iter()
            .map(|&payout| payout as f64)
            .sum::<f64>()
            / (stacks.len() - num_positive) as f64
    } else {
        0.0
    };

    let positive_stacks = positive_indices
        .iter()
        .map(|&index| stacks[index])
        .collect::<Vec<_>>();
    let positive_payouts = payouts[..num_positive]
        .iter()
        .map(|&payout| payout as f64)
        .collect::<Vec<_>>();
    let positive_values = icm_values(&positive_stacks, &positive_payouts)?;

    let mut values = vec![busted_value; stacks.len()];
    for (&index, &value) in positive_indices.iter().zip(&positive_values) {
        values[index] = value;
    }

    Ok(values)
}

fn icm_values(stacks: &[f64], payouts: &[f64]) -> Result<Vec<f64>, String> {
    let total_stack = stacks.iter().sum::<f64>();
    if total_stack <= 0.0 {
        return Err("At least one ICM stack must be positive".to_string());
    }

    let mut values = vec![0.0; stacks.len()];
    let mut remaining = (0..stacks.len()).collect::<Vec<_>>();
    icm_recursive(stacks, payouts, &mut remaining, 0, 1.0, &mut values);
    Ok(values)
}

fn icm_recursive(
    stacks: &[f64],
    payouts: &[f64],
    remaining: &mut Vec<usize>,
    place: usize,
    probability: f64,
    values: &mut [f64],
) {
    if remaining.is_empty() || place >= payouts.len() || probability == 0.0 {
        return;
    }

    if remaining.len() == 1 {
        values[remaining[0]] += probability * payouts[place];
        return;
    }

    let total_stack = remaining.iter().map(|&index| stacks[index]).sum::<f64>();
    for choice in 0..remaining.len() {
        let player = remaining[choice];
        let placement_probability = probability * stacks[player] / total_stack;
        values[player] += placement_probability * payouts[place];

        remaining.swap_remove(choice);
        icm_recursive(
            stacks,
            payouts,
            remaining,
            place + 1,
            placement_probability,
            values,
        );
        remaining.push(player);
        let last = remaining.len() - 1;
        remaining.swap(choice, last);
    }
}
