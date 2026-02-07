package expo.modules.keepbooknative

object KeepbookNativeRust {
  init {
    // The Rust shared library is built by the Gradle task in android/build.gradle.
    System.loadLibrary("keepbook_ffi")
  }

  external fun version(): String
  external fun initDemo(dataDir: String): String
  external fun listConnections(dataDir: String): String
  external fun listAccounts(dataDir: String): String
  external fun gitSync(
    repoDir: String,
    host: String,
    repo: String,
    sshUser: String,
    privateKeyPem: String,
    branch: String,
    authToken: String
  ): String
}
