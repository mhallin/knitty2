# Knitty2

> Dropbox for knitting machines

----

Knitty2 manages your knitting machine patterns. Patterns can be both uploaded
and downloaded to/from the machine.

## Features

* Reads or writes BMP, PNG, and JPEG images.
* Compacts the memory used to avoid gaps (fragmentation!) in memory, allowing
  you to use (almost) 100% of your machine's 32 kb memory.

## What Doesn't Work?

* Adding data to the memo display.
* Validating that the pattern fits within the machine's working memory.

## Platform Support

Only tested on macOS, but should work out of the box on both Windows and Linux
given that you have the software requirements listed below installed. Please let
me know if it does not - preferably with a pull request fixing the issue :-)

## What You Will Need

Hardware:

* Brother KH940 knitting machine. KH930 *might* work but is untested.
* USB FTDI cable connected to the machine.

Software:

* Rust compiler (https://rustup.rs)


# Installation Instructions

Clone this repository, build it, and run it with the standard Rust toolchain:

```sh
$ git clone git@github.com:mhallin/knitty2.git
$ cargo run
```

# Downloading Patterns from the Machine

```sh
# First, find your USB cable:
ls /dev/tty.usbserial-*

# This will use "patterns.bin" as the floppy drive image. It will
# be created if it does not exist.
cargo run -- emulate /dev/tty.usbserial-A7XTW5YZ patterns.bin
```

If this is the first time you run knitty2, you should download all patterns from
the machine first. On a KH940, this is done by entering ``CE``, ``552``,
``STEP``, ``1``, ``STEP``. When this is done, the machine should beep (as it
always does). Quit Knitty2 by pressing Control-C. Now, you need to unpack the
disk image into a folder.

```sh
# Export files from the floppy drive into a folder called patterns
cargo run -- export patterns.bin patterns
```

Now you can modify/add/remove patterns as much as you like. Just drop them in
the folder together with the other patterns.

# Uploading Patterns

When you're done with fiddling with the images, you should upload them:

```sh
# Import the files from a folder into a floppy disk image
cargo run -- import patterns.bin patterns

# Connect the USB-FTDI cable and emulate the floppy drive
cargo run -- emulate /dev/tty.usbserial-A7XTW5YZ patterns.bin
```

To load the patterns on the machine, enter ``CE``, ``551``, ``STEP``, ``1``,
``STEP`` and wait until it beeps.

# Acknowledgements

* The file format/memory dump file format documentation over at STG's
  [knittington] repository was a huge help in writing the parser/serializer.
* Steve Conklin's PDDemulate.py in [knitting_machine] was very useful in
  filling the gaps in Tandy's official floppy drive command documentation.

[knittington]: https://github.com/stg/knittington
[knitting_machine]: https://github.com/adafruit/knitting_machine
