fn days_count<const N: usize>(_days: &[u8; N], count_per_day: u32) -> u32 {
    u32::try_from(N).expect("Array size should be < u32::MAX") * count_per_day
}

#[test]
fn test_days_count() {
    let hours_in_day = 0..=23_u32;
    assert_eq!(
        hours_in_day.sum::<u32>(),
        276,
        "Sum of all hours 0 + 1 + 2 .. + 23 = 276"
    );

    // days in the form of e.g. 2.12.2021 & 3.12.2021
    let two_days = days_count(&[2, 3], 276);
    assert_eq!(two_days, 2 * 276);
}