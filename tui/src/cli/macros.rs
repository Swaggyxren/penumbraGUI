#[macro_export]
macro_rules! mtk_commands {
    ( $( $variant:ident ($ty:ty) ),+ $(,)? ) => {
        #[derive(clap::Subcommand, Debug)]
        pub enum Commands {
            $(
                #[command(
                    aliases = <$ty as $crate::cli::common::CommandMetadata>::aliases(),
                    visible_aliases = <$ty as $crate::cli::common::CommandMetadata>::visible_aliases(),
                    about = <$ty as $crate::cli::common::CommandMetadata>::about(),
                    long_about = <$ty as $crate::cli::common::CommandMetadata>::long_about(),
                    hide = <$ty as $crate::cli::common::CommandMetadata>::hide(),
                )]
                $variant($ty),
            )+
        }

        #[async_trait::async_trait]
        impl $crate::cli::MtkCommand for Commands {
            async fn run(
                &self,
                dev: &mut penumbra::Device,
                state: &mut $crate::cli::state::PersistedDeviceState,
            ) -> anyhow::Result<()> {
                match self {
                    $(
                        Commands::$variant(inner) => inner.run(dev, state).await,
                    )+
                }
            }
        }
    };
}

pub(crate) use mtk_commands;
