use vst::plugin::PluginParameters;
use crate::parameter_value_conversion::{f32_to_byte, byte_to_f32};
use vst::util::ParameterTransfer;

pub trait ParameterConversion<ParameterType>
    where ParameterType: From<i32>,
          ParameterType: Into<usize>,
          Self: PluginParameters
{
    #[inline]
    fn get_byte_parameter(&self, index: ParameterType) -> u8 {
        f32_to_byte(self.get_parameter_transfer().get_parameter(index.into()))
    }

    #[inline]
    fn set_byte_parameter(&self, index: ParameterType, value: u8) {
        self.get_parameter_transfer()
            .set_parameter(index.into(), byte_to_f32(value))
    }

    #[inline]
    fn get_exponential_scale_parameter(&self, index: ParameterType) -> Option<f32> {
        let x = self.get_parameter_transfer().get_parameter(index.into());
        const FACTOR: f32 = 20.0;
        if x == 0.0 {
            None
        } else {
            Some((FACTOR.powf(x) - 1.) * 5. / (FACTOR - 1.0))
        }
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
            .map(|i| self.get_byte_parameter(ParameterType::from(i as i32)))
            .collect()
    }

    fn deserialize_state(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(ParameterType::from(i as i32), *item);
        }
    }
}
