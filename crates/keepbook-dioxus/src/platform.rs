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

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JObject;
    use jni::JavaVM;

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
