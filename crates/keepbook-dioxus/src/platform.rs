/// The system's Material You seed, from the wallpaper on Android 12 and later.
#[cfg(target_os = "android")]
pub(crate) fn wallpaper_seed() -> Option<u32> {
    match android::wallpaper_seed() {
        Ok(seed) => seed,
        Err(error) => {
            eprintln!("Could not read the system accent color: {error}");
            None
        }
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) fn wallpaper_seed() -> Option<u32> {
    None
}

/// Picks dark system bar icons for light themes so they stay legible over the
/// app, which draws edge to edge behind the status and navigation bars.
#[cfg(target_os = "android")]
pub(crate) fn set_light_system_bars(light: bool) {
    dioxus::mobile::wry::prelude::dispatch(move |env, activity, _webview| {
        if let Err(error) = android::set_light_system_bars(env, activity, light) {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            eprintln!("Could not set the system bar appearance: {error}");
        }
    });
}

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JObject;
    use jni::{JNIEnv, JavaVM};

    /// Android 11 (API 30) added `WindowInsetsController`.
    const INSETS_CONTROLLER_SDK: i32 = 30;
    /// Android 8 (API 26) added light navigation bars.
    const LIGHT_NAVIGATION_BAR_SDK: i32 = 26;
    const APPEARANCE_LIGHT_STATUS_BARS: i32 = 0x8;
    const APPEARANCE_LIGHT_NAVIGATION_BARS: i32 = 0x10;
    const SYSTEM_UI_FLAG_LIGHT_STATUS_BAR: i32 = 0x2000;
    const SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR: i32 = 0x10;

    pub(super) fn set_light_system_bars(
        env: &mut JNIEnv,
        activity: &JObject,
        light: bool,
    ) -> jni::errors::Result<()> {
        let sdk = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()?;
        let window = env
            .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
            .l()?;
        if sdk >= INSETS_CONTROLLER_SDK {
            let controller = env
                .call_method(
                    &window,
                    "getInsetsController",
                    "()Landroid/view/WindowInsetsController;",
                    &[],
                )?
                .l()?;
            let mask = APPEARANCE_LIGHT_STATUS_BARS | APPEARANCE_LIGHT_NAVIGATION_BARS;
            let appearance = if light { mask } else { 0 };
            env.call_method(
                &controller,
                "setSystemBarsAppearance",
                "(II)V",
                &[appearance.into(), mask.into()],
            )?;
            return Ok(());
        }
        let decor = env
            .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
            .l()?;
        let mut mask = SYSTEM_UI_FLAG_LIGHT_STATUS_BAR;
        if sdk >= LIGHT_NAVIGATION_BAR_SDK {
            mask |= SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
        }
        let flags = env
            .call_method(&decor, "getSystemUiVisibility", "()I", &[])?
            .i()?;
        let flags = if light { flags | mask } else { flags & !mask };
        env.call_method(&decor, "setSystemUiVisibility", "(I)V", &[flags.into()])?;
        Ok(())
    }

    /// Android 12 (API 31) added the `system_accent1_*` dynamic color resources.
    const DYNAMIC_COLOR_SDK: i32 = 31;

    /// Reads `system_accent1_500`, the mid tone of the wallpaper-derived accent.
    pub(super) fn wallpaper_seed() -> jni::errors::Result<Option<u32>> {
        let context = ndk_context::android_context();
        // SAFETY: ndk-context holds the process's JavaVM and Activity for the
        // app's lifetime, and both pointers come from the Android glue.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let sdk = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()?;
        if sdk < DYNAMIC_COLOR_SDK {
            return Ok(None);
        }
        let resource = env
            .get_static_field("android/R$color", "system_accent1_500", "I")?
            .i()?;
        // SAFETY: the Activity reference stays valid while the app runs, and
        // `JObject` does not delete the reference when dropped.
        let activity = unsafe { JObject::from_raw(context.context().cast()) };
        let argb = env
            .call_method(&activity, "getColor", "(I)I", &[resource.into()])?
            .i()?;
        Ok(Some(argb as u32 & 0xff_ffff))
    }
}
