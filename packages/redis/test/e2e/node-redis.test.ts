import { describe, expect } from 'vitest'
import { ElectricRedis } from '../../src'
import { testWithClients, ItemRow } from '../support/test-context'

/**
 * E2E tests for ElectricRedis with node-redis client
 *
 * These tests require:
 * - PostgreSQL running on localhost:54321
 * - Electric running on localhost:3000
 * - Redis running on localhost:6379
 *
 * Run with: pnpm test:e2e
 */
describe(`ElectricRedis E2E - node-redis`, () => {
  testWithClients(
    `should sync items to Redis hash`,
    async ({
      nodeRedis,
      tableUrl,
      insertItems,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      // Insert test data
      await insertItems(
        { name: `Item 1`, value: 100 },
        { name: `Item 2`, value: 200 }
      )

      // Create sync
      const sync = new ElectricRedis({
        redis: nodeRedis,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      const items = sync.syncShape<ItemRow>(`items`, {
        shape: { table: tableUrl },
        structure: `hash`,
      })

      // Track inserts
      const inserted: string[] = []
      items.onInsert((key) => inserted.push(key))

      await sync.start()

      // Wait for sync
      await new Promise((r) => setTimeout(r, 2000))

      // Verify data in Redis using client directly
      const allData = await nodeRedis.hGetAll(`items`)
      const allItems = Object.entries(allData).map(([key, value]) => ({
        key,
        ...(JSON.parse(value) as ItemRow),
      }))
      expect(allItems).toHaveLength(2)

      const names = allItems.map((i) => i.name).sort()
      expect(names).toEqual([`Item 1`, `Item 2`])

      // Verify callbacks were called
      expect(inserted.length).toBeGreaterThanOrEqual(2)
    }
  )

  testWithClients(
    `should handle updates`,
    async ({
      nodeRedis,
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
        redis: nodeRedis,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      const items = sync.syncShape<ItemRow>(`items`, {
        shape: { table: tableUrl },
        structure: `hash`,
      })

      const updates: ItemRow[] = []
      items.onUpdate((_, data) => updates.push(data))

      await sync.start()
      await new Promise((r) => setTimeout(r, 1500))

      // Update the item
      await updateItem(id, { name: `Updated`, value: 999 })
      await new Promise((r) => setTimeout(r, 1500))

      // Verify update was synced using Redis client directly
      const allData = await nodeRedis.hGetAll(`items`)
      const allItems = Object.entries(allData).map(([key, value]) => ({
        key,
        ...(JSON.parse(value) as ItemRow),
      }))
      const updated = allItems.find((i) => i.id === id)
      expect(updated?.name).toBe(`Updated`)
      expect(updated?.value).toBe(999)
    }
  )

  testWithClients(
    `should handle deletes`,
    async ({
      nodeRedis,
      tableUrl,
      insertItems,
      deleteItem,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      const [id] = await insertItems({ name: `ToDelete`, value: 100 })

      const sync = new ElectricRedis({
        redis: nodeRedis,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      const items = sync.syncShape<ItemRow>(`items`, {
        shape: { table: tableUrl },
        structure: `hash`,
      })

      const deleted: string[] = []
      items.onDelete((key) => deleted.push(key))

      await sync.start()
      await new Promise((r) => setTimeout(r, 1500))

      // Verify item exists
      let allData = await nodeRedis.hGetAll(`items`)
      expect(Object.keys(allData)).toHaveLength(1)

      // Delete the item
      await deleteItem(id)
      await new Promise((r) => setTimeout(r, 1500))

      // Verify delete was synced
      allData = await nodeRedis.hGetAll(`items`)
      expect(Object.keys(allData)).toHaveLength(0)
      expect(deleted.length).toBeGreaterThanOrEqual(1)
    }
  )

  testWithClients(
    `should sync to Redis set`,
    async ({
      nodeRedis,
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
        redis: nodeRedis,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      sync.syncShape(`item_ids`, {
        shape: { table: tableUrl },
        structure: `set`,
      })

      await sync.start()
      await new Promise((r) => setTimeout(r, 2000))

      // Verify keys in set using Redis client directly
      const members = await nodeRedis.sMembers(`item_ids`)
      expect(members).toHaveLength(2)
    }
  )

  testWithClients(
    `should sync to Redis sorted set`,
    async ({
      nodeRedis,
      tableUrl,
      insertItems,
      clearRedis,
      electricUrl,
      registerSync,
    }) => {
      await clearRedis()

      await insertItems(
        { name: `Low`, value: 10 },
        { name: `High`, value: 100 },
        { name: `Mid`, value: 50 }
      )

      const sync = new ElectricRedis({
        redis: nodeRedis,
        electric: { url: `${electricUrl}/v1/shape` },
      })
      registerSync(sync)

      sync.syncShape<ItemRow>(`sorted_items`, {
        shape: { table: tableUrl },
        structure: `sorted_set`,
        score: `value`,
      })

      await sync.start()
      await new Promise((r) => setTimeout(r, 2000))

      // Verify sorted order using Redis client directly
      // Sorted set now stores IDs as members, not JSON
      const sortedWithScores = await nodeRedis.zRangeWithScores(
        `sorted_items`,
        0,
        -1
      )
      expect(sortedWithScores).toHaveLength(3)

      // Should be sorted by value (ascending)
      const scores = sortedWithScores.map((i) => i.score)
      expect(scores).toEqual([10, 50, 100])
    }
  )
})
