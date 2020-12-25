use vst::plugin::PluginParameters;
use vst::util::ParameterTransfer;

use super::parameter_value_conversion::{f32_to_byte, byte_to_f32, f32_to_bool, bool_to_f32, u14_to_f32, f32_to_u14};

// TODO can Parameter implement just from/into i32, and provide a default implementation for usize ?
pub trait ParameterConversion<ParameterType>
    where ParameterType: Into<i32> + From<i32>,
          Self: PluginParameters
{
    #[inline]
    fn get_byte_parameter(&self, index: ParameterType) -> u8 {
        f32_to_byte(self.get_parameter_transfer().get_parameter(index.into() as usize))
    }

    #[inline]
    fn set_byte_parameter(&self, index: ParameterType, value: u8) {
        self.get_parameter_transfer()
            .set_parameter(index.into() as usize, byte_to_f32(value))
    }

    #[inline]
    fn set_u14_parameter(&self, index: ParameterType, value: u16) {
        self.get_parameter_transfer()
            .set_parameter(index.into() as usize, u14_to_f32(value))
    }

    #[inline]
    fn get_u14_parameter(&self, index: ParameterType) -> u16 {
        f32_to_u14(self.get_parameter_transfer().get_parameter(index.into() as usize))
    }

    #[inline]
    fn get_exponential_scale_parameter(&self, index: ParameterType, max: f32, factor: f32) -> f32 {
        let x = self.get_parameter_transfer().get_parameter(index.into() as usize);
        (factor.powf(x) - 1.) * max / (factor - 1.0)
    }

    #[inline]
    fn get_bool_parameter(&self, index: ParameterType) -> bool {
        f32_to_bool(self.get_parameter_transfer().get_parameter(index.into() as usize))
    }

    #[inline]
    fn set_bool_parameter(&self, index: ParameterType, value: bool) {
        self.get_parameter_transfer()
            .set_parameter(index.into() as usize, bool_to_f32(value))
    }

    fn copy_parameter(&self, from_index: ParameterType, to_index: ParameterType) {
        self.set_parameter(to_index.into(), self.get_parameter(from_index.into()));
    }

    // the idea would be to provide an implementation of PluginParameters for the type implementing
    // this trait, but we can't do that, so it will have to be repeated by the implementing type.
    // at best, we force the implementing type to also implement PluginParameters
    // fn get_parameter(&self, index: i32) -> f32 {
    //     self.get_parameter_transfer().get_parameter(index as usize)
    // }

    fn get_parameter_transfer(&self) -> &ParameterTransfer ;


    fn get_parameter_count() -> usize ;

    fn serialize_state(&self) -> Vec<u8> {
        (0..Self::get_parameter_count())
            .map(|i | self.get_byte_parameter((i as i32).into()))
            .collect()
    }

    fn deserialize_state(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter((i as i32).into(), *item);
        }
    }
}
