// Learn more https://docs.expo.io/guides/customizing-metro
const { getDefaultConfig } = require('expo/metro-config');
const path = require('path');

const projectRoot = __dirname;
const tsLibRoot = path.resolve(projectRoot, '../ts/src');

/** @type {import('expo/metro-config').MetroConfig} */
const config = getDefaultConfig(projectRoot);

// ---------------------------------------------------------------------------
// 1. Watch the TS library directory so Metro can bundle files from it
// ---------------------------------------------------------------------------
config.watchFolders = [tsLibRoot];

// ---------------------------------------------------------------------------
// 2. Ensure node_modules from the frontend project are used when resolving
//    dependencies inside the TS library (e.g. decimal.js, date-fns, uuid)
// ---------------------------------------------------------------------------
config.resolver.nodeModulesPaths = [path.resolve(projectRoot, 'node_modules')];

// ---------------------------------------------------------------------------
// 3. Handle .js extension imports that point to .ts source files
//
// The TS library uses ESM with ".js" extensions in import specifiers
// (e.g. `from '../models/id.js'`) but the actual source files are .ts.
// Metro needs to resolve these to the .ts files.
// ---------------------------------------------------------------------------

// Make sure .ts and .tsx are in source extensions (they should be by default
// with Expo, but let's be explicit).
const sourceExts = config.resolver.sourceExts || [];
if (!sourceExts.includes('ts')) sourceExts.push('ts');
if (!sourceExts.includes('tsx')) sourceExts.push('tsx');
config.resolver.sourceExts = sourceExts;

// Custom resolver: when an import ending in .js is inside the TS library
// directory and points to a .ts file, rewrite the resolution.
const originalResolveRequest = config.resolver.resolveRequest;
config.resolver.resolveRequest = (context, moduleName, platform) => {
  // Only handle .js imports that originate from or resolve into the TS library
  if (moduleName.endsWith('.js')) {
    const tsModuleName = moduleName.slice(0, -3) + '.ts';

    // For relative imports from within the TS library, try .ts first
    if (moduleName.startsWith('.')) {
      try {
        if (originalResolveRequest) {
          return originalResolveRequest(context, tsModuleName, platform);
        }
        return context.resolveRequest(context, tsModuleName, platform);
      } catch {
        // Fall through to default resolution
      }
    }
  }

  // Handle @keepbook/* alias
  if (moduleName.startsWith('@keepbook/')) {
    const subpath = moduleName.slice('@keepbook/'.length);
    let resolved = path.join(tsLibRoot, subpath);

    // If the import has a .js extension, try .ts first
    if (resolved.endsWith('.js')) {
      const tsResolved = resolved.slice(0, -3) + '.ts';
      try {
        if (originalResolveRequest) {
          return originalResolveRequest(context, tsResolved, platform);
        }
        return context.resolveRequest(context, tsResolved, platform);
      } catch {
        // Fall through
      }
    }

    if (originalResolveRequest) {
      return originalResolveRequest(context, resolved, platform);
    }
    return context.resolveRequest(context, resolved, platform);
  }

  // Default resolution
  if (originalResolveRequest) {
    return originalResolveRequest(context, moduleName, platform);
  }
  return context.resolveRequest(context, moduleName, platform);
};

module.exports = config;
