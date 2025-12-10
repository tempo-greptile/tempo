/**
 * Generates MDX return type documentation from extract-sdk-types JSON output.
 *
 * Usage:
 *   bun scripts/sdk/generate-return-docs.ts <module> <function>
 *
 * Examples:
 *   bun scripts/sdk/generate-return-docs.ts dex placeFlip
 *   bun scripts/sdk/generate-return-docs.ts token getBalance
 *
 * Output:
 *   Writes MDX to snippets/sdk/returns/<module>.<function>.mdx
 */

import * as fs from 'node:fs'
import * as path from 'node:path'

const [, , moduleName, functionName] = process.argv

if (!moduleName || !functionName) {
  console.error(
    'Usage: bun scripts/sdk/generate-return-docs.ts <module> <function>',
  )
  console.error('Example: bun scripts/sdk/generate-return-docs.ts dex placeFlip')
  process.exit(1)
}

interface ReturnTypeInfo {
  type: string
  fields?: Record<string, { type: string; description?: string }>
}

interface TypeInfo {
  module: string
  function: string
  actionType: 'read' | 'write' | 'watch'
  hasSyncVariant: boolean
  returnType: ReturnTypeInfo
  syncReturnType?: ReturnTypeInfo
  callbackArgs?: ReturnTypeInfo
}

// Read the JSON file
const inputPath = path.join(
  process.cwd(),
  'snippets/sdk/types',
  `${moduleName}.${functionName}.json`,
)

if (!fs.existsSync(inputPath)) {
  console.error(`Input file not found: ${inputPath}`)
  console.error(
    `Run extract-sdk-types first: bun scripts/extract-sdk-types.ts ${moduleName} ${functionName}`,
  )
  process.exit(1)
}

const typeInfo: TypeInfo = JSON.parse(fs.readFileSync(inputPath, 'utf-8'))

/**
 * Generate a TypeScript type block with JSDoc comments for fields
 */
function generateTypeBlock(
  returnType: ReturnTypeInfo,
  typeName = 'ReturnType',
): string {
  const lines: string[] = ['```ts']

  if (returnType.fields && Object.keys(returnType.fields).length > 0) {
    lines.push(`type ${typeName} = {`)
    for (const [fieldName, fieldInfo] of Object.entries(returnType.fields)) {
      if (fieldInfo.description) {
        lines.push(`  /** ${fieldInfo.description} */`)
      }
      lines.push(`  ${fieldName}: ${fieldInfo.type}`)
    }
    lines.push('}')
  } else {
    // Simple type without fields
    lines.push(`type ${typeName} = ${returnType.type}`)
  }

  lines.push('```')
  return lines.join('\n')
}

// Generate MDX content based on action type
const mdxLines: string[] = []

if (typeInfo.actionType === 'watch') {
  // Watchers have callback args and return an unsubscribe function
  mdxLines.push('### Return Value')
  mdxLines.push('')
  mdxLines.push('```ts')
  mdxLines.push('type ReturnType = () => void')
  mdxLines.push('```')
  mdxLines.push('')
  mdxLines.push('Returns an unsubscribe function to stop watching.')
  mdxLines.push('')

  if (typeInfo.callbackArgs) {
    mdxLines.push('### Callback Arguments')
    mdxLines.push('')
    mdxLines.push(generateTypeBlock(typeInfo.callbackArgs, 'CallbackArgs'))
    mdxLines.push('')
  }
} else if (typeInfo.hasSyncVariant) {
  // Write actions with sync variant - show both
  mdxLines.push(`### \`${functionName}\``)
  mdxLines.push('')
  mdxLines.push(generateTypeBlock(typeInfo.returnType))
  mdxLines.push('')
  mdxLines.push('Returns a transaction hash.')
  mdxLines.push('')

  if (typeInfo.syncReturnType) {
    mdxLines.push(`### \`${functionName}Sync\``)
    mdxLines.push('')
    mdxLines.push(generateTypeBlock(typeInfo.syncReturnType))
    mdxLines.push('')
  }
} else {
  // Read actions - single return type
  mdxLines.push(generateTypeBlock(typeInfo.returnType))
  mdxLines.push('')
}

const mdxContent = mdxLines.join('\n')

// Write the MDX file
const outputDir = path.join(process.cwd(), 'snippets/sdk/returns')
fs.mkdirSync(outputDir, { recursive: true })

const outputPath = path.join(outputDir, `${moduleName}.${functionName}.mdx`)
fs.writeFileSync(outputPath, mdxContent)

console.log(`✓ Generated return type docs for ${moduleName}.${functionName}`)
console.log(`  Action type: ${typeInfo.actionType}`)
console.log(`  Output: ${outputPath}`)
