/**
 * Main example - demonstrates basic ElectricRedis usage with Hash structure
 *
 * This is the simplest example showing how to sync a Postgres table to Redis.
 *
 * Run with: pnpm dev
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

// Define the data type matching the database schema
interface Item {
  id: string
  name: string
  value: number
  created_at: string
}

async function main() {
  console.log(`ElectricRedis - Basic Hash Example`)
  console.log(`===================================\n`)

  // 1. Create Redis client
  const redis = createClient({
    url: process.env.REDIS_URL || `redis://localhost:6379`,
  })

  redis.on(`error`, (err) => console.error(`Redis error:`, err))

  // 2. Create ElectricRedis instance
  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis,
  })

  // 3. Define a shape to sync - defaults to Hash structure
  const items = sync.syncShape<Item>(`items`, {
    shape: { table: `items` },
    key: `id`,
    // structure: 'hash' is the default
  })

  // 4. Set up event listeners
  items.onInsert((key, item) => {
    console.log(`[INSERT] ${key}: ${item.name} (value: ${item.value})`)
  })

  items.onUpdate((key, item) => {
    console.log(`[UPDATE] ${key}: ${item.name} (value: ${item.value})`)
  })

  items.onDelete((key) => {
    console.log(`[DELETE] ${key}`)
  })

  // 5. Start syncing
  await sync.start()
  console.log(`Sync started! Waiting for data...\n`)

  // 6. Query the data after a brief delay
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // Read data using Redis client directly
  const allItemsData = await redis.hGetAll(`items`)
  const allItems = Object.entries(allItemsData).map(([key, value]) => ({
    key,
    ...(JSON.parse(value) as Item),
  }))
  console.log(`\nItems in Redis (${allItems.length} total):`)
  for (const item of allItems) {
    console.log(`  - ${item.name}: ${item.value}`)
  }

  // Get a single item by key
  if (allItems.length > 0) {
    const firstKey = allItems[0].key
    const firstValue = await redis.hGet(`items`, firstKey)
    const first = firstValue ? (JSON.parse(firstValue) as Item) : null
    console.log(`\nSingle item lookup: ${first?.name}`)
  }

  // 7. Keep running until interrupted
  console.log(`\nPress Ctrl+C to stop...`)

  const cleanup = async () => {
    console.log(`\nShutting down...`)
    await sync.stop()
    await redis.quit()
    process.exit(0)
  }

  process.on(`SIGINT`, cleanup)
  process.on(`SIGTERM`, cleanup)
}

main().catch((err) => {
  console.error(`Error:`, err)
  process.exit(1)
})
