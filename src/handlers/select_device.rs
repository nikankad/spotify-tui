use super::{
  super::app::{ActiveBlock, App},
  common_key_events,
};
use crate::event::Key;
use crate::network::IoEvent;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      app.set_current_route_state(Some(ActiveBlock::Library), None);
    }
    k if common_key_events::down_event(k) => {
      if let Some(p) = &app.devices
        && let Some(selected_device_index) = app.selected_device_index
      {
        let next_index =
          common_key_events::on_down_press_handler(&p.devices, Some(selected_device_index));
        app.selected_device_index = Some(next_index);
      };
    }
    k if common_key_events::up_event(k) => {
      if let Some(p) = &app.devices
        && let Some(selected_device_index) = app.selected_device_index
      {
        let next_index =
          common_key_events::on_up_press_handler(&p.devices, Some(selected_device_index));
        app.selected_device_index = Some(next_index);
      };
    }
    k if common_key_events::high_event(k) => {
      if let Some(_p) = &app.devices
        && let Some(_selected_device_index) = app.selected_device_index
      {
        let next_index = common_key_events::on_high_press_handler();
        app.selected_device_index = Some(next_index);
      };
    }
    k if common_key_events::middle_event(k) => {
      if let Some(p) = &app.devices
        && let Some(_selected_device_index) = app.selected_device_index
      {
        let next_index = common_key_events::on_middle_press_handler(&p.devices);
        app.selected_device_index = Some(next_index);
      };
    }
    k if common_key_events::low_event(k) => {
      if let Some(p) = &app.devices
        && let Some(_selected_device_index) = app.selected_device_index
      {
        let next_index = common_key_events::on_low_press_handler(&p.devices);
        app.selected_device_index = Some(next_index);
      };
    }
    Key::Enter => {
      if let (Some(devices), Some(index)) = (app.devices.clone(), app.selected_device_index)
        && let Some(device) = &devices.devices.get(index)
      {
        app.dispatch(IoEvent::TransferPlaybackToDevice(device.id.clone()));
      };
    }
    _ => {}
  }
}
