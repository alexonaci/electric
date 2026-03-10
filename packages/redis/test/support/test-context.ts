import { v4 as uuidv4 } from 'uuid'
import { Client } from 'pg'
import { inject, test } from 'vitest'
import { createClient, RedisClientType } from 'redis'
import Redis from 'ioredis'
import Valkey from 'iovalkey'
import { makePgClient } from './global-setup'
import { ElectricRedis } from '../../src'

export type ItemRow = { id: string; name: string; value: number }

/**
 * Test context with database client, Redis clients, and table helpers
 */
export const testWithClients = test.extend<{
  dbClient: Client
  nodeRedis: RedisClientType
  ioRedis: Redis
  ioValkey: Valkey
  tableName: string
  tableUrl: string
  insertItems: (...items: Partial<ItemRow>[]) => Promise<string[]>
  updateItem: (id: string, updates: Partial<ItemRow>) => Promise<void>
  deleteItem: (id: string) => Promise<void>
  clearRedis: () => Promise<void>
  electricUrl: string
  // Helper to track syncs for cleanup
  registerSync: (sync: ElectricRedis) => void
}>({
  dbClient: async ({}, use) => {
    const searchOption = `-csearch_path=${inject(`testPgSchema`)}`
    const client = makePgClient({ options: searchOption })
    await client.connect()
    await use(client)
    await client.end()
  },

  nodeRedis: async ({}, use) => {
    const client = createClient({ url: inject(`redisUrl`) })
    await client.connect()
    await use(client as RedisClientType)
    await client.quit()
  },

  ioRedis: async ({}, use) => {
    const client = new Redis(inject(`redisUrl`))
    await use(client)
    await client.quit()
  },

  ioValkey: async ({}, use) => {
    const client = new Valkey(inject(`redisUrl`))
    await use(client)
    await client.quit()
  },

  // Track syncs for cleanup before table drop
  registerSync: async ({}, use) => {
    const syncs: ElectricRedis[] = []
    await use((sync: ElectricRedis) => {
      syncs.push(sync)
    })
    // Stop all syncs before table is dropped
    for (const sync of syncs) {
      try {
        await sync.stop()
      } catch {
        // Ignore errors during cleanup
      }
    }
    // Small delay to let connections close
    await new Promise((r) => setTimeout(r, 100))
  },

  tableName: async ({ dbClient, registerSync: _registerSync }, use) => {
    const tableName = `items_${uuidv4().replace(/-/g, `_`)}`
    await dbClient.query(`
      CREATE TABLE ${tableName} (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        name TEXT NOT NULL,
        value INTEGER NOT NULL DEFAULT 0
      )
    `)
    await use(tableName)
    // registerSync cleanup happens before this due to fixture ordering
    await dbClient.query(`DROP TABLE IF EXISTS ${tableName}`)
  },

  tableUrl: async ({ tableName }, use) => {
    const schema = inject(`testPgSchema`)
    await use(`${schema}.${tableName}`)
  },

  insertItems: async ({ dbClient, tableName }, use) => {
    await use(async (...items: Partial<ItemRow>[]) => {
      const ids: string[] = []
      for (const item of items) {
        const id = item.id || uuidv4()
        await dbClient.query(
          `INSERT INTO ${tableName} (id, name, value) VALUES ($1, $2, $3)`,
          [id, item.name || `Item`, item.value || 0]
        )
        ids.push(id)
      }
      return ids
    })
  },

  updateItem: async ({ dbClient, tableName }, use) => {
    await use(async (id: string, updates: Partial<ItemRow>) => {
      const sets: string[] = []
      const values: unknown[] = []
      let i = 1

      if (updates.name !== undefined) {
        sets.push(`name = $${i++}`)
        values.push(updates.name)
      }
      if (updates.value !== undefined) {
        sets.push(`value = $${i++}`)
        values.push(updates.value)
      }

      values.push(id)
      await dbClient.query(
        `UPDATE ${tableName} SET ${sets.join(`, `)} WHERE id = $${i}`,
        values
      )
    })
  },

  deleteItem: async ({ dbClient, tableName }, use) => {
    await use(async (id: string) => {
      await dbClient.query(`DELETE FROM ${tableName} WHERE id = $1`, [id])
    })
  },

  clearRedis: async ({ nodeRedis }, use) => {
    await use(async () => {
      await nodeRedis.flushDb()
    })
  },

  electricUrl: async ({}, use) => {
    await use(inject(`electricUrl`))
  },
})
