/**
 * Stream Example - Change Log / Audit Trail
 *
 * This example demonstrates using Redis Streams with ElectricRedis.
 *
 * Streams are perfect for:
 * - Audit logs / change history
 * - Event sourcing
 * - Message queues with persistence
 * - Any append-only log of changes
 *
 * Run with: pnpm tsx src/stream-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

interface Item {
  id: string
  name: string
  value: number
  created_at: string
}

async function main() {
  console.log(`ElectricRedis - Stream (Change Log) Example`)
  console.log(`============================================\n`)

  // 1. Create Redis client
  const redis = createClient({
    url: process.env.REDIS_URL || `redis://localhost:6379`,
  })

  redis.on(`error`, (err) => console.error(`Redis error:`, err))
  await redis.connect()

  // 2. Create ElectricRedis instance
  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis,
  })

  // 3. Define a stream shape for the change log
  // Every insert creates a new stream entry (append-only)
  sync.syncShape<Item>(`items_changelog`, {
    shape: { table: `items` },
    key: `id`,
    structure: `stream`,
  })

  // 5. Start syncing
  await sync.start()
  console.log(`Sync started! Waiting for data...\n`)

  // 6. Query the stream after a brief delay
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // READ DATA USING REDIS CLIENT DIRECTLY
  // ==========================================

  // Get all entries from the stream using XRANGE
  const entries = await redis.xRange(`items_changelog`, `-`, `+`)
  console.log(`\nStream Entries (${entries.length} total):`)
  console.log(`-----------------------------------------`)
  entries.forEach((entry, index) => {
    const data = entry.message.data ? JSON.parse(entry.message.data) : null
    console.log(
      `  ${index + 1}. [${entry.id}] op=${entry.message.operation} key=${entry.message.key}`
    )
    if (data) {
      console.log(`       data: ${data.name}: ${data.value}`)
    }
  })

  // Get entries with count limit using XRANGE + COUNT
  const limited = await redis.xRange(`items_changelog`, `-`, `+`, { COUNT: 3 })
  console.log(`\nFirst 3 entries (via xRange with COUNT):`)
  limited.forEach((entry, i) => {
    console.log(`  ${i + 1}. [${entry.id}] ${entry.message.operation}`)
  })

  // Get stream length using XLEN
  const length = await redis.xLen(`items_changelog`)
  console.log(`\nStream length (xLen): ${length}`)

  // Get stream info using XINFO STREAM
  const info = await redis.xInfoStream(`items_changelog`)
  console.log(`Stream info:`, {
    length: info.length,
    firstEntry: info.firstEntry?.id,
    lastEntry: info.lastEntry?.id,
  })

  // 7. Keep running until interrupted
  console.log(`\n-----------------------------------------`)
  console.log(`Stream is live! Try inserting items in Postgres:`)
  console.log(
    `  INSERT INTO items (id, name, value) VALUES (gen_random_uuid()::text, 'LoggedItem', 100);`
  )
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
