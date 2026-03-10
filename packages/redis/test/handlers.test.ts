import { describe, it, expect, vi } from 'vitest'
import { handleOperation } from '../src/handlers'
import type { RedisMultiLike, InternalShapeConfig } from '../src/types'
import type { ScriptShaCache } from '../src/lua-scripts'

/**
 * Unit tests for Redis handlers
 *
 * These tests use mocked Redis clients to verify handler logic without
 * requiring an actual Redis server. Since we use Lua scripts, all
 * operations go through evalSha.
 */

// Mock script SHA cache - matches the script names in lua-scripts.ts
const mockScripts: ScriptShaCache = {
  HSET: `sha_hset`,
  HDEL: `sha_hdel`,
  HASH_UPDATE: `sha_hash_update`,
  SADD: `sha_sadd`,
  SREM: `sha_srem`,
  ZADD: `sha_zadd`,
  ZREM: `sha_zrem`,
  XADD: `sha_xadd`,
}

// Create a mock pipeline that records all operations
function createMockPipeline(): RedisMultiLike & {
  operations: Array<{ sha: string; keys: string[]; args: string[] }>
} {
  const operations: Array<{ sha: string; keys: string[]; args: string[] }> = []

  const pipeline = {
    operations,
    evalSha(sha1: string, options: { keys: string[]; arguments: string[] }) {
      operations.push({
        sha: sha1,
        keys: options.keys,
        args: options.arguments,
      })
      return this
    },
    exec: vi.fn().mockResolvedValue([]),
  }

  return pipeline as unknown as ReturnType<typeof createMockPipeline>
}

describe(`handlers`, () => {
  describe(`handleOperation`, () => {
    describe(`hash structure`, () => {
      it(`should insert data into hash via HSET script`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, name: `test` }

        handleOperation(
          pipeline,
          `hash`,
          `insert`,
          `key1`,
          data,
          `items`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_hset`,
          keys: [`items`, `key1`],
          args: [JSON.stringify(data)],
        })
      })

      it(`should delete from hash via HDEL script`, () => {
        const pipeline = createMockPipeline()

        handleOperation(
          pipeline,
          `hash`,
          `delete`,
          `key1`,
          null,
          `items`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_hdel`,
          keys: [`items`, `key1`],
          args: [],
        })
      })

      it(`should update hash with HASH_UPDATE script for partial updates`, () => {
        const pipeline = createMockPipeline()
        const data = { name: `updated` }

        handleOperation(
          pipeline,
          `hash`,
          `update`,
          `key1`,
          data,
          `items`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0].sha).toBe(`sha_hash_update`)
        expect(pipeline.operations[0].keys).toEqual([`items`, `key1`])
        expect(pipeline.operations[0].args).toEqual([JSON.stringify(data)])
      })
    })

    describe(`set structure`, () => {
      it(`should add key to set via SADD script`, () => {
        const pipeline = createMockPipeline()

        handleOperation(
          pipeline,
          `set`,
          `insert`,
          `key1`,
          { id: `1` },
          `item_ids`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_sadd`,
          keys: [`item_ids`],
          args: [`key1`],
        })
      })

      it(`should remove key from set via SREM script`, () => {
        const pipeline = createMockPipeline()

        handleOperation(
          pipeline,
          `set`,
          `delete`,
          `key1`,
          null,
          `item_ids`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_srem`,
          keys: [`item_ids`],
          args: [`key1`],
        })
      })

      it(`should not perform any operation on update`, () => {
        const pipeline = createMockPipeline()

        handleOperation(
          pipeline,
          `set`,
          `update`,
          `key1`,
          { id: `1` },
          `item_ids`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(0)
      })
    })

    describe(`sorted_set structure`, () => {
      it(`should add to sorted set via ZADD script`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, priority: 10 }
        const config: InternalShapeConfig = {
          shape: { table: `items` },
          structure: `sorted_set`,
          key: `id`,
          score: `priority`,
        }

        handleOperation(
          pipeline,
          `sorted_set`,
          `insert`,
          `key1`,
          data,
          `items`,
          mockScripts,
          config
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_zadd`,
          keys: [`items`],
          args: [`10`, `key1`], // score, member (key as ID)
        })
      })

      it(`should throw when score field is missing in config`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, name: `test` }

        expect(() => {
          handleOperation(
            pipeline,
            `sorted_set`,
            `insert`,
            `key1`,
            data,
            `items`,
            mockScripts
          )
        }).toThrow(`sorted_set handler requires 'score' field in config`)
      })

      it(`should throw when score field value is missing in data`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, name: `test` } // No priority field
        const config: InternalShapeConfig = {
          shape: { table: `items` },
          structure: `sorted_set`,
          key: `id`,
          score: `priority`,
        }

        expect(() => {
          handleOperation(
            pipeline,
            `sorted_set`,
            `insert`,
            `key1`,
            data,
            `items`,
            mockScripts,
            config
          )
        }).toThrow(`Score field 'priority' is missing in data for key 'key1'`)
      })

      it(`should handle update operation (ZADD updates score)`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, priority: 20 } // Updated score
        const config: InternalShapeConfig = {
          shape: { table: `items` },
          structure: `sorted_set`,
          key: `id`,
          score: `priority`,
        }

        handleOperation(
          pipeline,
          `sorted_set`,
          `update`,
          `key1`,
          data,
          `items`,
          mockScripts,
          config
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_zadd`,
          keys: [`items`],
          args: [`20`, `key1`],
        })
      })

      it(`should handle delete via ZREM script`, () => {
        const pipeline = createMockPipeline()
        const config: InternalShapeConfig = {
          shape: { table: `items` },
          structure: `sorted_set`,
          key: `id`,
          score: `priority`,
        }

        handleOperation(
          pipeline,
          `sorted_set`,
          `delete`,
          `key1`,
          undefined,
          `items`,
          mockScripts,
          config
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0]).toEqual({
          sha: `sha_zrem`,
          keys: [`items`],
          args: [`key1`],
        })
      })
    })

    describe(`stream structure`, () => {
      it(`should add entry via XADD script with data fields directly`, () => {
        const pipeline = createMockPipeline()
        const data = { id: `1`, name: `test`, value: 100 }

        handleOperation(
          pipeline,
          `stream`,
          `insert`,
          `key1`,
          data,
          `changes`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0].sha).toBe(`sha_xadd`)
        expect(pipeline.operations[0].keys).toEqual([`changes`])

        // XADD args: id, field1, value1, field2, value2, ...
        // Data fields should be stored directly, not wrapped in a "data" JSON blob
        const args = pipeline.operations[0].args
        expect(args[0]).toBe(`*`) // Auto-generated ID
        expect(args).toContain(`operation`)
        expect(args).toContain(`insert`)
        expect(args).toContain(`key`)
        expect(args).toContain(`key1`)
        // Verify data fields are stored directly
        expect(args).toContain(`id`)
        expect(args).toContain(`1`)
        expect(args).toContain(`name`)
        expect(args).toContain(`test`)
        expect(args).toContain(`value`)
        expect(args).toContain(`100`)
      })

      it(`should record delete operations in stream`, () => {
        const pipeline = createMockPipeline()

        handleOperation(
          pipeline,
          `stream`,
          `delete`,
          `key1`,
          null,
          `changes`,
          mockScripts
        )

        expect(pipeline.operations).toHaveLength(1)
        expect(pipeline.operations[0].sha).toBe(`sha_xadd`)

        const args = pipeline.operations[0].args
        expect(args).toContain(`operation`)
        expect(args).toContain(`delete`)
      })
    })
  })
})
