import { describe, it, expect } from 'vitest'
import { parseQuery, targetQuery } from './parseQuery'
describe('parseQuery', () => {
  it('ordered pairs', () => expect(parseQuery('a=1&b=2')).toEqual([['a', '1'], ['b', '2']]))
  it('duplicates preserved', () => expect(parseQuery('x=1&x=2')).toEqual([['x', '1'], ['x', '2']]))
  it('empty', () => expect(parseQuery('')).toEqual([]))
  it('targetQuery', () => {
    expect(targetQuery('/p?q=1')).toBe('q=1')
    expect(targetQuery('/p')).toBe('')
  })
})
