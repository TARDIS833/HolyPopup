export function filteredIndexes(verses, version) {
  return verses.flatMap((verse, index) => version === 'all' || verse.version === version ? [index] : []);
}
export function validSelection(verses, version, selected) {
  const indexes = filteredIndexes(verses, version);
  return indexes.includes(selected) ? selected : indexes[0];
}
export function fitFontSize(maxSize, fits) {
  if (fits(maxSize)) return maxSize;
  let low = 1, high = maxSize;
  while (high - low > 0.5) {
    const size = (low + high) / 2;
    if (fits(size)) low = size; else high = size;
  }
  return low;
}
