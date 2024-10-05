use core::fmt::Write;
use embedded_graphics::{
    mono_font::{
        iso_8859_13::{FONT_5X8, FONT_6X9},
        MonoTextStyle,
    },
    pixelcolor::BinaryColor,
    prelude::{Point, Size, *},
    primitives::{Circle, Line, PrimitiveStyle, Rectangle, StyledDrawable, Triangle},
    text::Text,
};
use heapless::{String, Vec};

use crate::{ButtonInput, DisplayType};

const VELO_MAX: i32 = 20;
const VELO_MIN: i32 = 1;
const MIDPOINT: i32 = 60;
const PLAYER_Y: i32 = 54;
const FRIEND_Y: i32 = 44;

const FRIEND_VELO_MAX: i32 = 19;
const FRIEND_VELO_MIN: i32 = 2;
const FRIEND_OFFSET_MAX: i32 = 250;
const FRIEND_OFFSET_MIN: i32 = -250;

const TICK_COUNTER_MAX: i32 = 40;

pub struct Friend {
    postion_offset: i32,
    velocity: i32,
}

pub struct Game<'a> {
    player_position_offset: i32,
    pub player_velocity: i32,

    friend_position_offset: i32,
    friend_velocity: i32,

    friends: Vec<Friend, 3>,

    sub_tick_counter: i32,

    score: i32,

    text_style: MonoTextStyle<'a, BinaryColor>,
}

enum Shift {
    Up,
    Down,
}

impl Game<'_> {
    pub fn new() -> Self {
        let mut friends: Vec<Friend, 3> = Vec::new();
        let _ = friends.push(Friend {
            postion_offset: 20,
            velocity: 4,
        });
        let _ = friends.push(Friend {
            postion_offset: -20,
            velocity: 4,
        });
        let _ = friends.push(Friend {
            postion_offset: -40,
            velocity: 4,
        });

        Self {
            player_velocity: 4,
            friend_velocity: 4,

            // This that are only updated with a tick
            player_position_offset: 0,
            friend_position_offset: -20,

            friends,

            // Tick regulator
            sub_tick_counter: 0,
            score: 0,
            text_style: MonoTextStyle::new(&FONT_6X9, BinaryColor::On),
        }
    }

    pub fn process_input(&mut self, input: ButtonInput) {
        match input {
            ButtonInput::Left => self.update_velocities(Shift::Down),
            ButtonInput::Center => (),
            ButtonInput::Right => self.update_velocities(Shift::Up),
        }
    }

    fn update_velocities(&mut self, shift: Shift) {
        self.player_velocity = new_player_velocity(self.player_velocity, shift);
    }

    pub fn tick(&mut self, random_bits: Vec<bool, 3>) {
        // Update positions
        self.player_position_offset = self.player_velocity - (VELO_MAX - VELO_MIN) / 2;

        self.sub_tick_counter = (self.sub_tick_counter + 1) % TICK_COUNTER_MAX;
        // Updating every now and then
        // Takes 3 random bits and updates velocity accordingly.
        if self.sub_tick_counter == 0 {
            self.friends
                .iter_mut()
                .enumerate()
                .for_each(|(idx, friend)| {
                    // Updating velocities according to the random bits
                    let random_bit = random_bits[idx];
                    match random_bit {
                        true => friend.velocity = (friend.velocity + 1).min(FRIEND_VELO_MAX),
                        false => friend.velocity = (friend.velocity - 1).max(FRIEND_VELO_MIN),
                    }

                    self.score += (13 - (self.player_position_offset - friend.postion_offset).abs())
                        .max(0)
                        * 10
                });
        }

        // Updating positions
        self.friends.iter_mut().for_each(|friend| {
            let projected_friend_position_offset =
                friend.postion_offset + (friend.velocity - self.player_velocity);

            if projected_friend_position_offset < FRIEND_OFFSET_MAX
                && projected_friend_position_offset > FRIEND_OFFSET_MIN
            {
                friend.postion_offset = projected_friend_position_offset;
            }
        });

        let projected_friend_position_offset =
            self.friend_position_offset + (self.friend_velocity - self.player_velocity);

        if projected_friend_position_offset < FRIEND_OFFSET_MAX
            && projected_friend_position_offset > FRIEND_OFFSET_MIN
        {
            self.friend_position_offset = projected_friend_position_offset;
        }
    }

    pub fn draw_player_character(&self, display: &mut DisplayType) {
        draw_character(
            &PrimitiveStyle::with_stroke(BinaryColor::On, 1),
            Point::new(MIDPOINT + self.player_position_offset, PLAYER_Y),
            display,
        );
    }

    pub fn draw_friend_characters(&self, display: &mut DisplayType) {
        self.friends.iter().for_each(|friend| {
            draw_character(
                &PrimitiveStyle::with_fill(BinaryColor::On),
                Point::new(MIDPOINT + friend.postion_offset, FRIEND_Y),
                display,
            );
        })
    }

    pub fn draw_score(&self, display: &mut DisplayType) {
        let mut score_string: String<6> = String::new();
        let _ = write!(score_string, "{:06}", self.score);
        let _text = Text::new(&score_string, Point::new(92, 9), self.text_style).draw(display);
    }
}

fn new_player_velocity(velocity: i32, shift: Shift) -> i32 {
    match shift {
        Shift::Up => (velocity + 1).min(VELO_MAX),
        Shift::Down => (velocity - 1).max(VELO_MIN),
    }
}

fn draw_character(style: &PrimitiveStyle<BinaryColor>, midpoint: Point, display: &mut DisplayType) {
    // Front wheel
    let _ = Circle::new(Point::new(midpoint.x + 3, midpoint.y), 8)
        .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Back wheel
    let _ = Circle::new(Point::new(midpoint.x - 10, midpoint.y), 8)
        .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Frame
    let _ = Triangle::new(
        Point::new(midpoint.x - 3, midpoint.y - 2),
        Point::new(midpoint.x + 3, midpoint.y - 2),
        Point::new(midpoint.x, midpoint.y + 4),
    )
    .draw_styled(style, display);

    // Frame back
    let _ = Triangle::new(
        Point::new(midpoint.x - 3, midpoint.y - 2),
        Point::new(midpoint.x - 7, midpoint.y + 4),
        Point::new(midpoint.x, midpoint.y + 4),
    )
    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Fork
    let _ = Line::new(
        Point::new(midpoint.x + 3, midpoint.y - 2),
        Point::new(midpoint.x + 7, midpoint.y + 4),
    )
    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Person
    let _ = Line::new(
        Point::new(midpoint.x - 3, midpoint.y - 2),
        Point::new(midpoint.x, midpoint.y - 6),
    )
    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Person head
    let _ = Circle::new(Point::new(midpoint.x - 2, midpoint.y - 8), 3)
        .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

    // Person arms
    let _ = Line::new(
        Point::new(midpoint.x, midpoint.y - 6),
        Point::new(midpoint.x + 2, midpoint.y - 2),
    )
    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);
}
