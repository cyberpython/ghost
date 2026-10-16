import { describe, it, expect } from 'vitest'
import { detectBodyType } from './detectBodyType'
describe('detectBodyType', () => {
  it('json by content-type', () => expect(detectBodyType('application/json; charset=utf-8', 'x', false)).toBe('json'))
  it('json by heuristic', () => expect(detectBodyType(undefined, '{"a":1}', false)).toBe('json'))
  it('xml by content-type', () => expect(detectBodyType('text/xml', '<a/>', false)).toBe('xml'))
  it('html by content-type', () => expect(detectBodyType('text/html', '<h1/>', false)).toBe('html'))
  it('binary flag wins', () => expect(detectBodyType('text/plain', '', true)).toBe('binary'))
  it('xml by <?xml', () => expect(detectBodyType(undefined, '<?xml version="1.0"?><a/>', false)).toBe('xml'))
  it('text fallback', () => expect(detectBodyType(undefined, 'hello world', false)).toBe('text'))
})
