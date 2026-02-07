import ExpoModulesCore

public class KeepbookNativeModule: Module {
  // Each module class must implement the definition function. The definition consists of components
  // that describes the module's functionality and behavior.
  // See https://docs.expo.dev/modules/module-api for more details about available components.
  public func definition() -> ModuleDefinition {
    // Sets the name of the module that JavaScript code will use to refer to the module. Takes a string as an argument.
    // Can be inferred from module's class name, but it's recommended to set it explicitly for clarity.
    // The module will be accessible from `requireNativeModule('KeepbookNative')` in JavaScript.
    Name("KeepbookNative")

    // Defines constant property on the module.
    Constant("PI") {
      Double.pi
    }

    // Defines event names that the module can send to JavaScript.
    Events("onChange")

    // Defines a JavaScript synchronous function that runs the native code on the JavaScript thread.
    Function("hello") {
      return "Hello world! 👋"
    }

    Function("version") {
      return "ios"
    }

    Function("demoDataDir") {
      let urls = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)
      guard let base = urls.first else { return "" }
      return base.appendingPathComponent("keepbook-demo/data").path
    }

    Function("gitRepoDir") {
      let urls = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)
      guard let base = urls.first else { return "" }
      return base.appendingPathComponent("keepbook-repo").path
    }

    Function("gitDataDir") {
      let urls = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)
      guard let base = urls.first else { return "" }
      return base.appendingPathComponent("keepbook-repo/data").path
    }

    Function("initDemo") { (_dataDir: String) in
      // TODO: build and link the Rust library for iOS.
      return "not implemented on ios yet"
    }

    Function("listConnections") { (_dataDir: String) in
      return "[]"
    }

    Function("listAccounts") { (_dataDir: String) in
      return "[]"
    }

    AsyncFunction("gitSync") { (_repoDir: String, _host: String, _repo: String, _sshUser: String, _privateKeyPem: String, _branch: String, _authToken: String) in
      return "not implemented on ios yet"
    }

    // Defines a JavaScript function that always returns a Promise and whose native code
    // is by default dispatched on the different thread than the JavaScript runtime runs on.
    AsyncFunction("setValueAsync") { (value: String) in
      // Send an event to JavaScript.
      self.sendEvent("onChange", [
        "value": value
      ])
    }

    // Enables the module to be used as a native view. Definition components that are accepted as part of the
    // view definition: Prop, Events.
    View(KeepbookNativeView.self) {
      // Defines a setter for the `url` prop.
      Prop("url") { (view: KeepbookNativeView, url: URL) in
        if view.webView.url != url {
          view.webView.load(URLRequest(url: url))
        }
      }

      Events("onLoad")
    }
  }
}
