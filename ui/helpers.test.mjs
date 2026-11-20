import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { filteredIndexes, validSelection, fitFontSize } from './helpers.mjs';
const verses = [{version:'WEB'}, {version:'사용자'}, {version:'WEB'}];
test('번역 필터는 전체 목록의 인덱스를 유지한다', () => {
  assert.deepEqual(filteredIndexes(verses, 'WEB'), [0, 2]);
  assert.equal(validSelection(verses, 'WEB', 1), 0);
  assert.equal(validSelection(verses, 'all', 1), 1);
  assert.equal(validSelection(verses, '없는 번역', 0), undefined);
});

test('영역에 맞는 최대 글자 크기를 찾는다', () => {
  assert.equal(fitFontSize(32, size => size <= 40), 32);
  assert.equal(fitFontSize(10, size => size <= 10), 10);
  assert.equal(fitFontSize(100, size => size <= 100), 100);
  const size = fitFontSize(80, value => value <= 23);
  assert.ok(size <= 23 && size > 22.5);
});
