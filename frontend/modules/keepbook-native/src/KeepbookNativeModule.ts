import { NativeModule, requireNativeModule } from 'expo';

import { KeepbookNativeModuleEvents } from './KeepbookNative.types';

declare class KeepbookNativeModule extends NativeModule<KeepbookNativeModuleEvents> {
  PI: number;
  hello(): string;
  version(): string;
  demoDataDir(): string;
  gitRepoDir(): string;
  gitDataDir(): string;
  initDemo(dataDir: string): string;
  listConnections(dataDir: string): string;
  listAccounts(dataDir: string): string;
  gitSync(
    repoDir: string,
    host: string,
    repo: string,
    sshUser: string,
    privateKeyPem: string,
    branch: string,
    authToken: string
  ): Promise<string>;
  setValueAsync(value: string): Promise<void>;
}

// This call loads the native module object from the JSI.
export default requireNativeModule<KeepbookNativeModule>('KeepbookNative');
