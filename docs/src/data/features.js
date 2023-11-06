// @ts-check

/**
 * @satisfies {import('@site/src/components/FeaturesSection/index').FeatureCardProps[]}
 */
const FEATURES = [
  {
    title: 'Fast and Fast',
    // SVG Retired from https://www.svgrepo.com/svg/301795/fast-delivery-truck
    // Under MIT License
    Icon: require('@site/static/assets/homepage-svg/fast.svg').default,
    description: (
      <>
        Rio is perceived fast, there's few reasons behind the speed. Rio is
        built in Rust and also built over ANSI handler and parser is built from
        Alacritty terminal's VTE. Rio owns a renderer called Sugarloaf that
        contains a "sugar" architecture (inspired by React Redux state machine)
        created for minimal and quick interactions in render steps using
        performance at highest.
      </>
    ),
  },
  {
    title: 'Built with Rust',
    // SVG Retired from https://www.svgrepo.com/svg/232008/crab
    // Under CC0 License
    Icon: require('@site/static/assets/homepage-svg/rust.svg').default,
    description: (
      <>
        Rust language provides a mechanism called “ownership” that has a set of
        rules that are checked at compilation time, if these ownership rules are
        violated, the program won’t compile. This mechanism enforce memory
        safety without needing a garbage collector. The ownership rules don’t
        have a run time impact on performance either.
      </>
    ),
  },
  {
    // SVG Retired from https://www.svgrepo.com/svg/439109/color-wheel
    // Under MIT License
    // The SVG suffered changes from the original file
    title: '24-bit true color',
    Icon: require('@site/static/assets/homepage-svg/colors.svg').default,
    description: (
      <>
        Regular terminals use 256-color palette, which is configured at start
        and is a 666-cube of colors, each of them defined as a 24-bit (888 RGB)
        color, which means it can only display 256 different colors in the
        terminal while "true color" means that you can display 16 million
        different colors at the same time.
      </>
    ),
  },
  {
    // SVG Retired from https://www.svgrepo.com/svg/444458/gui-pictures
    // Under MIT License
    title: 'Image protocols',
    Icon: require('@site/static/assets/homepage-svg/images.svg').default,
    description: (
      <>
        Rio terminal implements iTerm2 and Kitty image protocols. Both protocols
        provide the ability of display images within the terminal. Using a
        similar mechanism, it can also facilitate file transfers over any
        transport (such as ssh or telnet), even in a non-8-bit-clean
        environment.
      </>
    ),
  },
  // {
  //   // SVG Retired from https://www.svgrepo.com/svg/267831/typography-font
  //   // Under CC0 License
  //   title: 'Font ligatures',
  //   Icon: require('@site/static/assets/homepage-svg/ligatures.svg').default,
  //   description: (
  //     <>
  //       Ligatures are special characters in a font that combine two or more characters into one. They were originally invented by scribes as a way to increase handwriting speed by combining commonly used characters. Often code editors provide font ligatures support as a way to improve readability of common expressions or operators. For example, <code>!=</code> would be replaced with <code>≠</code> in a ligatured font.
  //     </>
  //   ),
  // },
  {
    // SVG Retired from https://www.svgrepo.com/svg/454420/browser-chrome-google
    // Under CC0 License
    title: 'Support to WebGPU',
    Icon: require('@site/static/assets/homepage-svg/webgpu.svg').default,
    description: (
      <>
        Rio uses an implementation of WebGPU for use outside of a browser and as
        backend for firefox's WebGPU implementation. WebGPU allows for more
        efficient usage of modern GPU's than WebGL. Applications using WPGU run
        natively on Vulkan, Metal, DirectX 11/12, and OpenGL ES; and browsers
        via WebAssembly on WebGPU and WebGL2.
      </>
    ),
  },
];

export default FEATURES;
