//! `Array.prototype.sort(comparefn)` と同じ並べ替え（V8 の third_party/v8/builtins/array-sort.tq の TimSort）。
//!
//! 比べる関数が一貫した順序を返すなら、どの安定な並べ替えでも結果は同じになる。読み取りには NaN を含む値や、
//! 空のハンドルを順序に含めない比べ方があり、そのときの並びは比べる順で変わるので、V8 と同じ手順で比べる。
//! Rust の `sort_by` は一貫しない比べ方で panic することがあるので使わない。

/// ギャロップに入るまでに、片方の並びが続けて勝つ回数
const MIN_GALLOP: usize = 7;

struct TimSort<'s, T, F> {
    work: &'s mut [T],
    compare: F,
    min_gallop: usize,
    /// まだ併合していない並び（先頭と長さ）
    runs: Vec<(usize, usize)>,
}

impl<T: Clone, F: FnMut(&T, &T) -> f64> TimSort<'_, T, F> {
    /// comparefn の結果。NaN は 0（ECMAScript の SortCompare）
    fn order(&mut self, a: &T, b: &T) -> f64 {
        let value = (self.compare)(a, b);
        if value.is_nan() { 0.0 } else { value }
    }

    fn sort(&mut self) {
        let length = self.work.len();
        if length < 2 {
            return;
        }
        let mut remaining = length;
        let mut low = 0;
        let min_run = min_run_length(remaining);
        while remaining != 0 {
            let mut current = self.count_and_make_run(low, low + remaining);
            // 短い並びは、二分挿入で min(min_run, remaining) の長さにする
            if current < min_run {
                let forced = min_run.min(remaining);
                self.binary_insertion_sort(low, low + current, low + forced);
                current = forced;
            }
            self.runs.push((low, current));
            self.merge_collapse();
            low += current;
            remaining -= current;
        }
        self.merge_force_collapse();
    }

    /// low から始まる、昇順か狭義の降順の並びの長さ。降順なら逆にする
    fn count_and_make_run(&mut self, low_arg: usize, high: usize) -> usize {
        let low = low_arg + 1;
        if low == high {
            return 1;
        }
        let mut run_length = 2;
        let element_low = self.work[low].clone();
        let element_low_pre = self.work[low - 1].clone();
        let descending = self.order(&element_low, &element_low_pre) < 0.0;
        let mut previous = element_low;
        for index in low + 1..high {
            let current = self.work[index].clone();
            let order = self.order(&current, &previous);
            if descending {
                if order >= 0.0 {
                    break;
                }
            } else if order < 0.0 {
                break;
            }
            previous = current;
            run_length += 1;
        }
        if descending {
            self.work[low_arg..low_arg + run_length].reverse();
        }
        run_length
    }

    /// [low, start) は並んでいるので、[start, high) を 1 つずつ二分探索で差し込む
    fn binary_insertion_sort(&mut self, low: usize, start_arg: usize, high: usize) {
        let mut start = if low == start_arg { start_arg + 1 } else { start_arg };
        while start < high {
            let mut left = low;
            let mut right = start;
            let pivot = self.work[start].clone();
            while left < right {
                let mid = left + ((right - left) >> 1);
                let element = self.work[mid].clone();
                if self.order(&pivot, &element) < 0.0 {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            self.work[left..=start].rotate_right(1);
            start += 1;
        }
    }

    /// run_length(n - 2) > run_length(n - 1) + run_length(n)
    fn invariant(&self, n: usize) -> bool {
        if n < 2 {
            return true;
        }
        self.runs[n - 2].1 > self.runs[n - 1].1 + self.runs[n].1
    }

    fn merge_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if !self.invariant(n + 1) || !self.invariant(n) {
                if self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
                self.merge_at(n);
            } else if self.runs[n].1 <= self.runs[n + 1].1 {
                self.merge_at(n);
            } else {
                break;
            }
        }
    }

    fn merge_force_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if n > 0 && self.runs[n - 1].1 < self.runs[n + 1].1 {
                n -= 1;
            }
            self.merge_at(n);
        }
    }

    fn merge_at(&mut self, i: usize) {
        let stack_size = self.runs.len();
        let (mut base_a, mut length_a) = self.runs[i];
        let (base_b, mut length_b) = self.runs[i + 1];
        self.runs[i].1 = length_a + length_b;
        if i + 3 == stack_size {
            self.runs[i + 1] = self.runs[i + 2];
        }
        self.runs.truncate(stack_size - 1);
        // b の先頭が a のどこに入るか。それより前の a はそのまま
        let key_right = self.work[base_b].clone();
        let k = self.gallop_right(false, &key_right, base_a, length_a, 0, &[]);
        base_a += k;
        length_a -= k;
        if length_a == 0 {
            return;
        }
        // a の末尾が b のどこに入るか。それより後の b はそのまま
        let key_left = self.work[base_a + length_a - 1].clone();
        length_b = self.gallop_left(false, &key_left, base_b, length_b, length_b - 1, &[]);
        if length_b == 0 {
            return;
        }
        if length_a <= length_b {
            self.merge_low(base_a, length_a, base_b, length_b);
        } else {
            self.merge_high(base_a, length_a, base_b, length_b);
        }
    }

    fn element(&self, temp: bool, index: usize, temp_array: &[T]) -> T {
        if temp { temp_array[index].clone() } else { self.work[index].clone() }
    }

    /// key より小さい要素の数（同じ値があれば、その最も左の位置）。array は temp なら temp_array、でなければ work
    fn gallop_left(&mut self, temp: bool, key: &T, base: usize, length: usize, hint: usize, temp_array: &[T]) -> usize {
        let mut last_offset: isize = 0;
        let mut offset: isize = 1;
        let hint_element = self.element(temp, base + hint, temp_array);
        let (hint, length) = (hint as isize, length as isize);
        if self.order(&hint_element, key) < 0.0 {
            // a[hint] < key: a[hint + last_offset] < key <= a[hint + offset] になるまで右へ
            let max_offset = length - hint;
            while offset < max_offset {
                let element = self.element(temp, (base as isize + hint + offset) as usize, temp_array);
                if self.order(&element, key) >= 0.0 {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            offset = offset.min(max_offset);
            last_offset += hint;
            offset += hint;
        } else {
            // key <= a[hint]: a[hint - offset] < key <= a[hint - last_offset] になるまで左へ
            let max_offset = hint + 1;
            while offset < max_offset {
                let element = self.element(temp, (base as isize + hint - offset) as usize, temp_array);
                if self.order(&element, key) < 0.0 {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            offset = offset.min(max_offset);
            let previous = last_offset;
            last_offset = hint - offset;
            offset = hint - previous;
        }
        // a[last_offset] < key <= a[offset] の間を二分探索する
        last_offset += 1;
        while last_offset < offset {
            let middle = last_offset + ((offset - last_offset) >> 1);
            let element = self.element(temp, (base as isize + middle) as usize, temp_array);
            if self.order(&element, key) < 0.0 {
                last_offset = middle + 1;
            } else {
                offset = middle;
            }
        }
        offset as usize
    }

    /// key 以下の要素の数（同じ値があれば、その最も右の次の位置）
    fn gallop_right(
        &mut self,
        temp: bool,
        key: &T,
        base: usize,
        length: usize,
        hint: usize,
        temp_array: &[T],
    ) -> usize {
        let mut last_offset: isize = 0;
        let mut offset: isize = 1;
        let hint_element = self.element(temp, base + hint, temp_array);
        let (hint, length) = (hint as isize, length as isize);
        if self.order(key, &hint_element) < 0.0 {
            // key < a[hint]: a[hint - offset] <= key < a[hint - last_offset] になるまで左へ
            let max_offset = hint + 1;
            while offset < max_offset {
                let element = self.element(temp, (base as isize + hint - offset) as usize, temp_array);
                if self.order(key, &element) >= 0.0 {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            offset = offset.min(max_offset);
            let previous = last_offset;
            last_offset = hint - offset;
            offset = hint - previous;
        } else {
            // a[hint] <= key: a[hint + last_offset] <= key < a[hint + offset] になるまで右へ
            let max_offset = length - hint;
            while offset < max_offset {
                let element = self.element(temp, (base as isize + hint + offset) as usize, temp_array);
                if self.order(key, &element) < 0.0 {
                    break;
                }
                last_offset = offset;
                offset = (offset << 1) + 1;
            }
            offset = offset.min(max_offset);
            last_offset += hint;
            offset += hint;
        }
        last_offset += 1;
        while last_offset < offset {
            let middle = last_offset + ((offset - last_offset) >> 1);
            let element = self.element(temp, (base as isize + middle) as usize, temp_array);
            if self.order(key, &element) < 0.0 {
                offset = middle;
            } else {
                last_offset = middle + 1;
            }
        }
        offset as usize
    }

    /// a（短い方）を一時の配列に移し、前から併合する
    fn merge_low(&mut self, base_a: usize, length_a: usize, base_b: usize, length_b: usize) {
        let mut length_a = length_a;
        let mut length_b = length_b;
        let temp: Vec<T> = self.work[base_a..base_a + length_a].to_vec();
        let mut dest = base_a;
        let mut cursor_temp = 0;
        let mut cursor_b = base_b;
        self.work[dest] = self.work[cursor_b].clone();
        dest += 1;
        cursor_b += 1;
        enum End {
            Succeed,
            CopyB,
        }
        let end = 'merge: {
            length_b -= 1;
            if length_b == 0 {
                break 'merge End::Succeed;
            }
            if length_a == 1 {
                break 'merge End::CopyB;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                // どちらかが続けて勝つまで、1 つずつ比べる
                loop {
                    let b = self.work[cursor_b].clone();
                    if self.order(&b, &temp[cursor_temp]) < 0.0 {
                        self.work[dest] = b;
                        dest += 1;
                        cursor_b += 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[dest] = temp[cursor_temp].clone();
                        dest += 1;
                        cursor_temp += 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 1 {
                            break 'merge End::CopyB;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    }
                }
                // 片方が勝ち続ける間はギャロップで進める
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP || wins_b >= MIN_GALLOP || first {
                    first = false;
                    min_gallop = (min_gallop - 1).max(1);
                    self.min_gallop = min_gallop;
                    let key = self.work[cursor_b].clone();
                    wins_a = self.gallop_right(true, &key, cursor_temp, length_a, 0, &temp);
                    if wins_a > 0 {
                        self.work[dest..dest + wins_a].clone_from_slice(&temp[cursor_temp..cursor_temp + wins_a]);
                        dest += wins_a;
                        cursor_temp += wins_a;
                        length_a -= wins_a;
                        if length_a == 1 {
                            break 'merge End::CopyB;
                        }
                        // 比べ方が一貫しなければ 0 になることがある
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    self.work[dest] = self.work[cursor_b].clone();
                    dest += 1;
                    cursor_b += 1;
                    length_b -= 1;
                    if length_b == 0 {
                        break 'merge End::Succeed;
                    }
                    let key = temp[cursor_temp].clone();
                    wins_b = self.gallop_left(false, &key, cursor_b, length_b, 0, &[]);
                    if wins_b > 0 {
                        copy_within(self.work, cursor_b, dest, wins_b);
                        dest += wins_b;
                        cursor_b += wins_b;
                        length_b -= wins_b;
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    self.work[dest] = temp[cursor_temp].clone();
                    dest += 1;
                    cursor_temp += 1;
                    length_a -= 1;
                    if length_a == 1 {
                        break 'merge End::CopyB;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if length_a > 0 {
                    self.work[dest..dest + length_a].clone_from_slice(&temp[cursor_temp..cursor_temp + length_a]);
                }
            }
            End::CopyB => {
                // a の最後の 1 つは併合の最後に来る
                copy_within(self.work, cursor_b, dest, length_b);
                self.work[dest + length_b] = temp[cursor_temp].clone();
            }
        }
    }

    /// b（短い方）を一時の配列に移し、後ろから併合する
    fn merge_high(&mut self, base_a: usize, length_a: usize, base_b: usize, length_b: usize) {
        let mut length_a = length_a;
        let mut length_b = length_b;
        let temp: Vec<T> = self.work[base_b..base_b + length_b].to_vec();
        let mut dest = (base_b + length_b - 1) as isize;
        let mut cursor_temp = length_b as isize - 1;
        let mut cursor_a = (base_a + length_a - 1) as isize;
        let at = |index: isize| index as usize;
        self.work[at(dest)] = self.work[at(cursor_a)].clone();
        dest -= 1;
        cursor_a -= 1;
        enum End {
            Succeed,
            CopyA,
        }
        let end = 'merge: {
            length_a -= 1;
            if length_a == 0 {
                break 'merge End::Succeed;
            }
            if length_b == 1 {
                break 'merge End::CopyA;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let a = self.work[at(cursor_a)].clone();
                    if self.order(&temp[at(cursor_temp)], &a) < 0.0 {
                        self.work[at(dest)] = a;
                        dest -= 1;
                        cursor_a -= 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[at(dest)] = temp[at(cursor_temp)].clone();
                        dest -= 1;
                        cursor_temp -= 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 1 {
                            break 'merge End::CopyA;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP || wins_b >= MIN_GALLOP || first {
                    first = false;
                    min_gallop = (min_gallop - 1).max(1);
                    self.min_gallop = min_gallop;
                    let key = temp[at(cursor_temp)].clone();
                    let k = self.gallop_right(false, &key, base_a, length_a, length_a - 1, &[]);
                    wins_a = length_a - k;
                    if wins_a > 0 {
                        dest -= wins_a as isize;
                        cursor_a -= wins_a as isize;
                        copy_within(self.work, at(cursor_a + 1), at(dest + 1), wins_a);
                        length_a -= wins_a;
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    self.work[at(dest)] = temp[at(cursor_temp)].clone();
                    dest -= 1;
                    cursor_temp -= 1;
                    length_b -= 1;
                    if length_b == 1 {
                        break 'merge End::CopyA;
                    }
                    let key = self.work[at(cursor_a)].clone();
                    let k = self.gallop_left(true, &key, 0, length_b, length_b - 1, &temp);
                    wins_b = length_b - k;
                    if wins_b > 0 {
                        dest -= wins_b as isize;
                        cursor_temp -= wins_b as isize;
                        let from = at(cursor_temp + 1);
                        self.work[at(dest + 1)..at(dest + 1) + wins_b].clone_from_slice(&temp[from..from + wins_b]);
                        length_b -= wins_b;
                        if length_b == 1 {
                            break 'merge End::CopyA;
                        }
                        // 比べ方が一貫しなければ 0 になることがある
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    self.work[at(dest)] = self.work[at(cursor_a)].clone();
                    dest -= 1;
                    cursor_a -= 1;
                    length_a -= 1;
                    if length_a == 0 {
                        break 'merge End::Succeed;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if length_b > 0 {
                    let to = at(dest - (length_b as isize - 1));
                    self.work[to..to + length_b].clone_from_slice(&temp[..length_b]);
                }
            }
            End::CopyA => {
                // b の最初の 1 つは併合の先頭に来る
                dest -= length_a as isize;
                cursor_a -= length_a as isize;
                copy_within(self.work, at(cursor_a + 1), at(dest + 1), length_a);
                self.work[at(dest)] = temp[at(cursor_temp)].clone();
            }
        }
    }
}

/// 同じ配列の中の重なりうる範囲を写す（V8 の Copy と同じく、後ろへ写すときは後ろから）
fn copy_within<T: Clone>(array: &mut [T], from: usize, to: usize, length: usize) {
    if from < to {
        for index in (0..length).rev() {
            array[to + index] = array[from + index].clone();
        }
    } else {
        for index in 0..length {
            array[to + index] = array[from + index].clone();
        }
    }
}

/// 併合する並びの最小の長さ（64 未満の配列は全体を二分挿入で並べる）
fn min_run_length(length: usize) -> usize {
    let mut n = length;
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// `items.sort(compare)`。compare は負・0・正を返す（NaN は 0 とみなす）
pub fn sort_by<T: Clone>(items: &mut [T], compare: impl FnMut(&T, &T) -> f64) {
    TimSort { work: items, compare, min_gallop: MIN_GALLOP, runs: Vec::new() }.sort();
}

#[cfg(test)]
mod tests {
    use super::sort_by;

    #[test]
    fn sorts_like_v8_with_inconsistent_comparisons() {
        // Node.js 24 で `indices.sort((a, b) => values[a] - values[b])` とした並び（-1 は NaN）。
        // NaN との比べは 0 になり順序が一貫しないので、併合とギャロップの手順まで同じでないと一致しない
        let values = [
            3, 11, 6, 10, -1, 2, 11, -1, 0, -1, 11, 8, -1, 5, 10, -1, 1, -1, 8, 9, 8, -1, 1, 4, 9, 10, 2, 6, 3, 7, 8,
            4, 2, -1, 6, 3, 0, 2, -1, 7, 11, 5, 7, 2, 7, -1, 9, 5, 6, 3, 3, 0, 8, 2, 3, 5, 4, -1, 1, 3, 7, 8, 3, 5, 9,
            11, 6, 4, 10, 4, 10, 2, 4, -1, -1, 9, 6, 2, 7, 10, 8, -1, 4, 8, -1, 4, 7, 6, 5, 0, 7, 7, 7, -1, 5, 9, 1, 3,
            -1, 0, 9, 0, 9, 2, 1, 6, 7, -1, 1, 1, 1, 5, 0, -1, -1, -1, 0, 3, 2, 5, 11, 7, 3, 9, 1, 8, -1, -1, 2, 7, 10,
            10, 1, 6, 9, 7, 10, 11, 10, 11, 6, -1, 0, 0, 0, 10, -1, 1, 0, 2,
        ]
        .map(|value| if value < 0 { f64::NAN } else { f64::from(value) });
        let expected = [
            8, 36, 16, 22, 5, 26, 32, 37, 0, 28, 35, 23, 31, 13, 2, 27, 34, 29, 11, 18, 20, 30, 19, 24, 3, 14, 1, 4, 6,
            7, 9, 10, 12, 15, 17, 21, 25, 33, 38, 51, 58, 43, 53, 71, 49, 50, 54, 59, 62, 56, 67, 69, 72, 41, 47, 55,
            63, 48, 66, 39, 42, 44, 60, 52, 61, 46, 64, 40, 45, 57, 75, 68, 70, 65, 73, 74, 89, 99, 101, 112, 96, 104,
            108, 109, 110, 77, 103, 97, 82, 85, 88, 94, 111, 76, 87, 105, 78, 86, 90, 91, 92, 106, 80, 83, 100, 102,
            79, 81, 84, 93, 95, 98, 107, 113, 114, 115, 116, 142, 143, 144, 148, 124, 132, 147, 118, 128, 149, 117,
            122, 119, 133, 140, 121, 129, 135, 125, 123, 134, 131, 138, 145, 120, 126, 127, 130, 136, 137, 139, 141,
            146,
        ];
        let mut indices: Vec<usize> = (0..values.len()).collect();
        sort_by(&mut indices, |a, b| values[*a] - values[*b]);
        assert_eq!(indices, expected);
    }

    #[test]
    fn keeps_equal_elements_in_order() {
        let mut items = [(2, 'a'), (1, 'b'), (2, 'c'), (1, 'd')];
        sort_by(&mut items, |a, b| f64::from(a.0) - f64::from(b.0));
        assert_eq!(items, [(1, 'b'), (1, 'd'), (2, 'a'), (2, 'c')]);
    }
}
