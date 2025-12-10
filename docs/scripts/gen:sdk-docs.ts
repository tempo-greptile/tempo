/**
 * Generates MDX parameter and return type documentation for SDK actions.
 *
 * Usage:
 *   bun run gen:sdk-docs [modules...]
 *
 * Examples:
 *   bun run gen:sdk-docs           # Generate for all modules
 *   bun run gen:sdk-docs token     # Generate for token module only
 *   bun run gen:sdk-docs amm dex   # Generate for amm and dex modules
 *
 * This script:
 *   1. Discovers all actions from existing viem/wagmi documentation pages
 *   2. Runs scripts/sdk/extract-sdk-types.ts for each action
 *   3. Generates MDX snippets for params and returns
 */

import { execSync } from 'node:child_process'
import * as fs from 'node:fs'
import * as path from 'node:path'

const VIEM_ACTIONS_DIR = path.join(process.cwd(), 'pages/sdk/typescript/viem')
const WAGMI_ACTIONS_DIR = path.join(
  process.cwd(),
  'pages/sdk/typescript/wagmi/actions',
)
const SDK_TYPES_DIR = path.join(process.cwd(), 'snippets/sdk/types')
const PARAMS_DIR = path.join(process.cwd(), 'snippets/sdk/params')
const RETURNS_DIR = path.join(process.cwd(), 'snippets/sdk/returns')

const SDK_MODULES = ['amm', 'dex', 'fee', 'policy', 'reward', 'token']

// Parse command line arguments
const args = process.argv.slice(2)
const requestedModules = args.length > 0 ? args : SDK_MODULES

// Validate requested modules
for (const mod of requestedModules) {
  if (!SDK_MODULES.includes(mod)) {
    console.error(`Unknown module: ${mod}`)
    console.error(`Available modules: ${SDK_MODULES.join(', ')}`)
    process.exit(1)
  }
}

interface ParamInfo {
  name: string
  type: string
  optional: boolean
  description?: string
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
  parameters: ParamInfo[]
  returnType: ReturnTypeInfo
  syncReturnType?: ReturnTypeInfo
  callbackArgs?: ReturnTypeInfo
}

/**
 * Discover actions from existing documentation pages.
 * Looks at both viem and wagmi action docs to find module.function patterns.
 */
function discoverActionsFromDocs(moduleName: string): string[] {
  const functions = new Set<string>()

  // Check viem actions
  const viemFiles = fs.readdirSync(VIEM_ACTIONS_DIR)
  for (const file of viemFiles) {
    if (file.startsWith(`${moduleName}.`) && file.endsWith('.mdx')) {
      const funcName = file.replace(`${moduleName}.`, '').replace('.mdx', '')
      functions.add(funcName)
    }
  }

  // Check wagmi actions
  const wagmiFiles = fs.readdirSync(WAGMI_ACTIONS_DIR)
  for (const file of wagmiFiles) {
    if (file.startsWith(`${moduleName}.`) && file.endsWith('.mdx')) {
      const funcName = file.replace(`${moduleName}.`, '').replace('.mdx', '')
      functions.add(funcName)
    }
  }

  return [...functions].sort()
}

/**
 * Normalize description by replacing newlines with spaces
 */
function normalizeDescription(desc: string | undefined): string | undefined {
  if (!desc) return undefined
  return desc.replace(/\n+/g, ' ').trim()
}

/**
 * Format a type string for display in markdown.
 * Removes backticks from template literal types like `0x${string}`.
 */
function formatType(type: string): string {
  return type.replace(/`(0x\$\{string\})`/g, '$1')
}

// Enhanced descriptions for common parameters
const ENHANCED_DESCRIPTIONS: Record<
  string,
  { viem?: string; wagmi?: string; common?: string }
> = {
  account: {
    viem: 'Account that will be used to send the transaction.',
    wagmi:
      'Account that will be used to send the transaction. Defaults to connected Wagmi account.',
  },
  feeToken: {
    common:
      'Fee token for the transaction. Can be a TIP-20 token address or ID.',
  },
  feePayer: {
    common:
      'Fee payer for the transaction. Can be a [Viem Account](https://viem.sh/docs/accounts/local/privateKeyToAccount), or `true` if a [Fee Payer Service](/sdk/typescript/server/handler.feePayer) will be used.',
  },
  gas: {
    common: 'Gas limit for the transaction.',
  },
  maxFeePerGas: {
    common: 'Max fee per gas for the transaction.',
  },
  maxPriorityFeePerGas: {
    common: 'Max priority fee per gas for the transaction.',
  },
  nonce: {
    common: 'Nonce for the transaction.',
  },
  nonceKey: {
    common:
      "Nonce key for the transaction. Use `'random'` to generate a random nonce key.",
  },
  validBefore: {
    common: 'Unix timestamp before which the transaction must be included.',
  },
  validAfter: {
    common: 'Unix timestamp after which the transaction can be included.',
  },
  throwOnReceiptRevert: {
    common:
      'Whether to throw an error if the transaction receipt indicates a revert. Only applicable to `*Sync` actions.',
  },
}

function generateParamDoc(param: ParamInfo, isWagmi: boolean): string {
  const optionalSuffix = param.optional ? ' (optional)' : ''
  const formattedType = formatType(param.type)
  const lines = [
    `### ${param.name}${optionalSuffix}`,
    '',
    `- **Type:** \`${formattedType}\``,
    '',
  ]

  const enhanced = ENHANCED_DESCRIPTIONS[param.name]
  let description = param.description
  if (enhanced) {
    if (isWagmi && enhanced.wagmi) {
      description = enhanced.wagmi
    } else if (!isWagmi && enhanced.viem) {
      description = enhanced.viem
    } else if (enhanced.common) {
      description = enhanced.common
    }
  }

  const normalizedDesc = normalizeDescription(description)
  if (normalizedDesc) {
    lines.push(normalizedDesc)
    lines.push('')
  }

  return lines.join('\n')
}

function generateParamsDocs(typeInfo: TypeInfo, isWagmi: boolean): string {
  return typeInfo.parameters
    .map((param) => generateParamDoc(param, isWagmi))
    .join('\n')
}

function generateTypeBlock(
  returnType: ReturnTypeInfo,
  typeName = 'ReturnType',
): string {
  const lines: string[] = ['```ts']

  if (returnType.fields && Object.keys(returnType.fields).length > 0) {
    lines.push(`type ${typeName} = {`)
    for (const [fieldName, fieldInfo] of Object.entries(returnType.fields)) {
      const desc = normalizeDescription(fieldInfo.description)
      if (desc) {
        lines.push(`  /** ${desc} */`)
      }
      lines.push(`  ${fieldName}: ${formatType(fieldInfo.type)}`)
    }
    lines.push('}')
  } else {
    lines.push(`type ${typeName} = ${formatType(returnType.type)}`)
  }

  lines.push('```')
  return lines.join('\n')
}

function generateReturnDocs(typeInfo: TypeInfo): string {
  const mdxLines: string[] = []

  if (typeInfo.actionType === 'watch') {
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
    mdxLines.push(`### \`${typeInfo.function}\``)
    mdxLines.push('')
    mdxLines.push(generateTypeBlock(typeInfo.returnType))
    mdxLines.push('')
    mdxLines.push('Returns a transaction hash.')
    mdxLines.push('')

    if (typeInfo.syncReturnType) {
      mdxLines.push(`### \`${typeInfo.function}Sync\``)
      mdxLines.push('')
      mdxLines.push(generateTypeBlock(typeInfo.syncReturnType))
      mdxLines.push('')
    }
  } else {
    mdxLines.push(generateTypeBlock(typeInfo.returnType))
    mdxLines.push('')
  }

  return mdxLines.join('\n')
}

// Ensure output directories exist
fs.mkdirSync(SDK_TYPES_DIR, { recursive: true })
fs.mkdirSync(PARAMS_DIR, { recursive: true })
fs.mkdirSync(RETURNS_DIR, { recursive: true })

console.log(`Generating docs for modules: ${requestedModules.join(', ')}\n`)

// Step 1: Discover actions from existing documentation pages
const actions: Array<{ module: string; function: string }> = []
for (const mod of requestedModules) {
  const functions = discoverActionsFromDocs(mod)
  console.log(`Found ${functions.length} actions in ${mod}`)
  for (const func of functions) {
    actions.push({ module: mod, function: func })
  }
}

console.log(`\nTotal: ${actions.length} actions to process\n`)

// Step 2: Extract types for each action
console.log('Extracting types...')
let extractedCount = 0
for (const action of actions) {
  try {
    execSync(
      `bun scripts/sdk/extract-sdk-types.ts ${action.module} ${action.function}`,
      { stdio: 'pipe' },
    )
    extractedCount++
  } catch {
    console.error(`  ✗ ${action.module}.${action.function} (extract failed)`)
  }
}
console.log(`Extracted ${extractedCount} action types\n`)

// Step 3: Generate snippets from extracted types
console.log('Generating snippets...')
let generatedCount = 0

// Get all JSON files for the requested modules
const jsonFiles = fs
  .readdirSync(SDK_TYPES_DIR)
  .filter((f) => f.endsWith('.json'))
  .filter((f) => {
    const mod = f.split('.')[0]
    return mod && requestedModules.includes(mod)
  })
  .sort()

for (const jsonFile of jsonFiles) {
  const inputPath = path.join(SDK_TYPES_DIR, jsonFile)
  const typeInfo: TypeInfo = JSON.parse(fs.readFileSync(inputPath, 'utf-8'))
  const baseName = `${typeInfo.module}.${typeInfo.function}`

  // Generate viem params
  const viemParams = generateParamsDocs(typeInfo, false)
  fs.writeFileSync(path.join(PARAMS_DIR, `${baseName}.mdx`), viemParams)

  // Generate wagmi params
  const wagmiParams = generateParamsDocs(typeInfo, true)
  fs.writeFileSync(path.join(PARAMS_DIR, `${baseName}.wagmi.mdx`), wagmiParams)

  // Generate return docs
  const returnDocs = generateReturnDocs(typeInfo)
  fs.writeFileSync(path.join(RETURNS_DIR, `${baseName}.mdx`), returnDocs)

  console.log(`  ✓ ${baseName}`)
  generatedCount++
}

console.log(`\n✓ Generated docs for ${generatedCount} actions`)
console.log(`  Types: ${SDK_TYPES_DIR}`)
console.log(`  Params: ${PARAMS_DIR}`)
console.log(`  Returns: ${RETURNS_DIR}`)
