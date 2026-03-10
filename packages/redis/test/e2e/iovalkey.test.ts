import { describe, expect } from 'vitest'
import { ElectricRedis } from '../../src'
import { testWithClients, ItemRow } from '../support/test-context'

/**
 * E2E tests for ElectricRedis with iovalkey client
 */
describe(`ElectricRedis E2E - iovalkey`, () => {
  testWithClients(
    `should sync items to Redis hash`,
    async ({
      ioValkey,
      tableUrl,
      insertItems,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      await insertItems(
        { name: `Item 1`, value: 100 },
        { name: `Item 2`, value: 200 }
      )

      const sync = new ElectricRedis({
        redis: ioValkey,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      const items = sync.syncShape<ItemRow>(`items`, {
        shape: { table: tableUrl },
        structure: `hash`,
      })

      const inserted: string[] = []
      items.onInsert((key) => inserted.push(key))

      await sync.start()
      await new Promise((r) => setTimeout(r, 2000))

      // Read using iovalkey client (same API as ioredis)
      const allData = await ioValkey.hgetall(`items`)
      const allItems = Object.entries(allData).map(([key, value]) => ({
        key,
        ...(JSON.parse(value) as ItemRow),
      }))
      expect(allItems).toHaveLength(2)
      expect(inserted.length).toBeGreaterThanOrEqual(2)
    }
  )

  testWithClients(
    `should handle updates`,
    async ({
      ioValkey,
      tableUrl,
      insertItems,
      updateItem,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      const [id] = await insertItems({ name: `Original`, value: 100 })

      const sync = new ElectricRedis({
        redis: ioValkey,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      sync.syncShape<ItemRow>(`items`, {
        shape: { table: tableUrl },
        structure: `hash`,
      })

      await sync.start()
      await new Promise((r) => setTimeout(r, 1500))

      await updateItem(id, { name: `Updated` })
      await new Promise((r) => setTimeout(r, 1500))

      // Read using iovalkey client
      const allData = await ioValkey.hgetall(`items`)
      const allItems = Object.entries(allData).map(([key, value]) => ({
        key,
        ...(JSON.parse(value) as ItemRow),
      }))
      const updated = allItems.find((i) => i.id === id)
      expect(updated?.name).toBe(`Updated`)
    }
  )

  testWithClients(
    `should sync to Redis set`,
    async ({
      ioValkey,
      tableUrl,
      insertItems,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      await insertItems({ name: `Item 1`, value: 100 })

      const sync = new ElectricRedis({
        redis: ioValkey,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      sync.syncShape(`item_ids`, {
        shape: { table: tableUrl },
        structure: `set`,
      })

      await sync.start()
      await new Promise((r) => setTimeout(r, 2000))

      // Read using iovalkey client
      const members = await ioValkey.smembers(`item_ids`)
      expect(members).toHaveLength(1)
    }
  )

  testWithClients(
    `should sync to Redis sorted set`,
    async ({
      ioValkey,
      tableUrl,
      insertItems,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      await insertItems(
        { name: `Low`, value: 10 },
        { name: `High`, value: 100 }
      )

      const sync = new ElectricRedis({
        redis: ioValkey,
        electric: { url: `${electricUrl}/v1/shape` },
        batchSize: 100,
      })
      registerSync(sync)

      sync.syncShape<ItemRow>(`sorted_items`, {
        shape: { table: tableUrl },
        structure: `sorted_set`,
        score: `value`,
      })

      await sync.start()
      await new Promise((r) => setTimeout(r, 2000))

      // Read using iovalkey client - WITHSCORES returns flat array
      const sorted = await ioValkey.zrange(`sorted_items`, 0, -1, `WITHSCORES`)
      expect(sorted.length).toBe(4) // 2 items * 2 (id + score)
      // Scores should be ascending: 10, 100
      expect(parseFloat(sorted[1])).toBe(10)
      expect(parseFloat(sorted[3])).toBe(100)
    }
  )
})
