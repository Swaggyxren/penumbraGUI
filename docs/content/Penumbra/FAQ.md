## Frequently asked questions (FAQ)

### What's the difference between Penumbra and Antumbra?

[[Penumbra]] is a Rust crate (library) for interacting with MediaTek devices.
It's useful for developers who want to use it on their own applications.
It's like a backend.

[[Antumbra]] is a CLI and TUI written in Rust, and used [[Penumbra]] as its backend.
It's like a frontend.

### I get an error about "No signer available" during DA upload or during DA SLA challenge

Your device needs additional auth to proceed. Without exploits, this means that most of the times you'll need paid auth.
While penumbra includes some SLA keys, they are not enough to cover all devices, especially Xiaomi, OnePlus and some other OEMs.

THIS IS NOT A BUG! You should not report this, rather look elsewhere for auth.

### I get error 0x7017 / 0x7024 on my device when trying to upload DA1

This means your device has DAA on!
DAA (Download Agent Authentication) is a security mechanism in MediaTek devices that stops users from loading "unauthorized" [[Download Agent|DAs]].
This means that you can only load the official DA provided by your device manufacturer.

Some OEMs might invalidate DAs that used to work on the same device after an update, in which case you'll either need to get a new DA or if possible downgrade the preloader.

Error 0x7024 (usually a preloader error) means that the DA file you provided failed the signature verification.

Error 0x7017 is a BROM error, meaning that **you need to provide an Auth file**.

In both cases, you will need to get the necessary files from official firmware or paid tools.

### What if I instead get error "SendDA command failed with status: 0x1D0D"

This means your device also has SLA on!

Unfortunately for you, this means you're in BROM and you'll need to get online auth from paid tools, or hope for future exploits.

### I get an error about DA SLA, what should I do?

If you get an error like "DA SLA signature rejected (dummy), can't proceed!", it means your DA implements DA SLA, a security mesaure like BROM SLA, where a challenge is signed by the host and the DA verifies it.
This is to ensure only authorized hosts can perform operations on the device.

Unfortunately, you'll also need paid auth too for this.

### My device is not being detected

If you're on Linux, try setting up your udev rules and running as sudo.

If you're on Windows, you'll need ot fight with your OS a bit more, and install the proper driver. Generally, you can use either WinUSB or LibUSB if you need `Linecode` exploit, or stick to stock MediaTek USB drivers otherwise. For USB, I suggest using [Zadig](https://zadig.akeo.ie/).

### Where can I ask questions?

For asking questions, check the [discussions section](https://github.com/shomykohai/penumbra/discussions) on Penumbra repo.
