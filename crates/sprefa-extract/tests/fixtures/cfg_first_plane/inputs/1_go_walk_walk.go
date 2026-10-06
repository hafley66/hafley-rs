package p

func walk(items []int) int {
	total := 0
	for _, item := range items {
		if item < 0 {
			continue
		}
		if item > 100 {
			break
		}
		total += item
	}
	if total == 0 {
		return -1
	}
	return total
}
