import { describe, it, expect } from 'vitest'
import { chunk } from '../src/utils'
import { LUA_SCRIPTS } from '../src/lua-scripts'

describe(`utils`, () => {
  describe(`chunk`, () => {
    it(`should split array into chunks of specified size`, () => {
      const arr = [1, 2, 3, 4, 5, 6, 7]
      const chunks = chunk(arr, 3)

      expect(chunks).toEqual([[1, 2, 3], [4, 5, 6], [7]])
    })

    it(`should handle empty array`, () => {
      const chunks = chunk([], 3)
      expect(chunks).toEqual([])
    })

    it(`should handle array smaller than chunk size`, () => {
      const arr = [1, 2]
      const chunks = chunk(arr, 5)

      expect(chunks).toEqual([[1, 2]])
    })

    it(`should handle array exactly matching chunk size`, () => {
      const arr = [1, 2, 3]
      const chunks = chunk(arr, 3)

      expect(chunks).toEqual([[1, 2, 3]])
    })

    it(`should handle chunk size of 1`, () => {
      const arr = [1, 2, 3]
      const chunks = chunk(arr, 1)

      expect(chunks).toEqual([[1], [2], [3]])
    })
  })

  describe(`LUA_SCRIPTS`, () => {
    it(`should have HASH_UPDATE script for atomic field merge`, () => {
      expect(typeof LUA_SCRIPTS.HASH_UPDATE).toBe(`string`)
      expect(LUA_SCRIPTS.HASH_UPDATE).toContain(`redis.call`)
      expect(LUA_SCRIPTS.HASH_UPDATE).toContain(`HGET`)
      expect(LUA_SCRIPTS.HASH_UPDATE).toContain(`HSET`)
      expect(LUA_SCRIPTS.HASH_UPDATE).toContain(`cjson.decode`)
      expect(LUA_SCRIPTS.HASH_UPDATE).toContain(`cjson.encode`)
    })

    it(`should have all required scripts`, () => {
      const requiredScripts = [
        `HSET`,
        `HDEL`,
        `HASH_UPDATE`,
        `SADD`,
        `SREM`,
        `ZADD`,
        `ZREM`,
        `XADD`,
      ]

      for (const script of requiredScripts) {
        expect(LUA_SCRIPTS).toHaveProperty(script)
        expect(typeof LUA_SCRIPTS[script as keyof typeof LUA_SCRIPTS]).toBe(
          `string`
        )
      }
    })
  })
})
